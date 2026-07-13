use std::{io::Write, time::Duration};

use anyhow::{Context, anyhow, bail, ensure};
use pandar_core::{
    FirmwareAcknowledgement, FirmwareCatalogEntry, FirmwareControlMetadata,
    FirmwareTerminalOutcome, PrinterFirmwareModule,
};
use serde::de::DeserializeOwned;

use super::{
    callbacks::{FirmwareCallback, FirmwareTunnel},
    model::{FirmwareSendOutcome, StudioFirmwareCommand},
};
use crate::studio_status::acknowledgement_callback_json;

mod types;
use types::{
    ErrorResponse, ExecuteCommand, ExecutePhase, ExecuteRequest, ExecuteResponse,
    FirmwareStateResponse, PreparedResponse, RefreshRequest, RefreshResponse,
};

const FIRMWARE_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const FIRMWARE_REFRESH_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BODY: usize = 64 * 1024;

pub(super) struct FirmwareHttpClient {
    hub_url: String,
    token: String,
}

pub(super) struct HttpSendResult {
    pub(super) outcome: FirmwareSendOutcome,
    pub(super) callback: Option<FirmwareCallback>,
}

impl FirmwareHttpClient {
    pub(super) fn new(hub_url: String, token: String) -> Self {
        Self { hub_url, token }
    }

    pub(super) fn send(
        &self,
        dev_id: &str,
        printer_id: &str,
        command: &StudioFirmwareCommand,
        tunnel: FirmwareTunnel,
        diagnostics: &mut impl Write,
    ) -> HttpSendResult {
        let result =
            crate::runtime().block_on(self.send_async(dev_id, printer_id, command, tunnel));
        match result {
            Ok(result) => result,
            Err(SendError::Prepare(error)) => {
                write_diagnostic(diagnostics, "firmware prepare failed", &error);
                HttpSendResult {
                    outcome: FirmwareSendOutcome::PrePublishFailure,
                    callback: None,
                }
            }
            Err(SendError::Execute(error)) => {
                write_diagnostic(diagnostics, "firmware execute outcome unknown", &error);
                HttpSendResult {
                    outcome: FirmwareSendOutcome::OutcomeUnknown,
                    callback: None,
                }
            }
        }
    }

    pub(super) fn catalog(&self, printer_id: &str) -> anyhow::Result<Vec<FirmwareCatalogEntry>> {
        crate::runtime().block_on(async {
            let response = self
                .request(reqwest::Method::GET, printer_id, "firmware")?
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .context("send Hub firmware catalog request")?;
            decode_success::<FirmwareStateResponse>(response, "firmware catalog")
                .await
                .map(|response| response.catalog)
        })
    }

    pub(super) fn refresh(
        &self,
        printer_id: &str,
        sequence_id: &str,
    ) -> anyhow::Result<Vec<PrinterFirmwareModule>> {
        crate::runtime().block_on(async {
            let response = self
                .request(reqwest::Method::POST, printer_id, "firmware/refresh")?
                .timeout(FIRMWARE_REFRESH_HTTP_TIMEOUT)
                .json(&RefreshRequest { sequence_id })
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .context("send Hub firmware version refresh request")?;
            let response =
                decode_success::<RefreshResponse>(response, "firmware version refresh").await?;
            ensure!(
                !response.modules.is_empty(),
                "Hub firmware version refresh returned no modules"
            );
            Ok(response.modules)
        })
    }

    async fn send_async(
        &self,
        dev_id: &str,
        printer_id: &str,
        command: &StudioFirmwareCommand,
        tunnel: FirmwareTunnel,
    ) -> Result<HttpSendResult, SendError> {
        let metadata = FirmwareControlMetadata::from(command);
        let prepare = self
            .request(reqwest::Method::POST, printer_id, "firmware/prepare")
            .map_err(SendError::Prepare)?
            .json(&metadata)
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .context("send Hub firmware prepare request")
            .map_err(SendError::Prepare)?;
        let prepared = decode_success::<PreparedResponse>(prepare, "firmware prepare")
            .await
            .map_err(SendError::Prepare)?;
        if prepared.prepared_token.is_empty() {
            return Err(SendError::Prepare(anyhow!("empty firmware prepared token")));
        }

        let execute = self
            .request(reqwest::Method::POST, printer_id, "firmware/execute")
            .map_err(SendError::Execute)?
            .json(&ExecuteRequest {
                prepared_token: &prepared.prepared_token,
                command: ExecuteCommand::from(command),
            })
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .context("send Hub firmware execute request")
            .map_err(SendError::Execute)?;
        classify_execute(execute, dev_id, command, tunnel).await
    }

    fn request(
        &self,
        method: reqwest::Method,
        printer_id: &str,
        suffix: &str,
    ) -> anyhow::Result<reqwest::RequestBuilder> {
        let url = firmware_url(&self.hub_url, printer_id, suffix)?;
        let client = reqwest::Client::builder()
            .timeout(FIRMWARE_HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .context("build Hub firmware client")?;
        Ok(client.request(method, url).bearer_auth(&self.token))
    }
}

enum SendError {
    Prepare(anyhow::Error),
    Execute(anyhow::Error),
}

async fn classify_execute(
    response: reqwest::Response,
    dev_id: &str,
    command: &StudioFirmwareCommand,
    tunnel: FirmwareTunnel,
) -> Result<HttpSendResult, SendError> {
    let status = response.status();
    let bytes = response_bytes(response, "firmware execute")
        .await
        .with_context(|| format!("Hub firmware execute returned HTTP {status}"))
        .map_err(SendError::Execute)?;
    if !status.is_success() {
        if !status.is_client_error() && !status.is_server_error() {
            return Err(SendError::Execute(anyhow!(
                "Hub firmware execute returned HTTP {status} after preparation"
            )));
        }
        let error = serde_json::from_slice::<ErrorResponse>(&bytes)
            .context("decode Hub firmware execute error response")
            .with_context(|| format!("Hub firmware execute returned HTTP {status}"))
            .map_err(SendError::Execute)?;
        if error.phase == Some(ExecutePhase::PrePublishFailure) {
            return Ok(HttpSendResult {
                outcome: FirmwareSendOutcome::PrePublishFailure,
                callback: None,
            });
        }
        return Err(SendError::Execute(anyhow!(
            "Hub firmware execute returned HTTP {status} without a safe pre-publish failure"
        )));
    }
    let response = serde_json::from_slice::<ExecuteResponse>(&bytes)
        .context("decode Hub firmware execute response")
        .with_context(|| format!("Hub firmware execute returned HTTP {status}"))
        .map_err(SendError::Execute)?;
    match (response.phase, response.outcome) {
        (ExecutePhase::PrePublishFailure, _) => Ok(HttpSendResult {
            outcome: FirmwareSendOutcome::PrePublishFailure,
            callback: None,
        }),
        (
            phase @ (ExecutePhase::Acknowledged | ExecutePhase::Rejected),
            Some(FirmwareTerminalOutcome::Acknowledged { acknowledgement }),
        ) if acknowledgement_matches(command, &acknowledgement) => Ok(HttpSendResult {
            outcome: if phase == ExecutePhase::Acknowledged {
                FirmwareSendOutcome::Acknowledged
            } else {
                FirmwareSendOutcome::Rejected
            },
            callback: Some(FirmwareCallback {
                dev_id: dev_id.to_owned(),
                tunnel,
                message: acknowledgement_callback_json(
                    &acknowledgement,
                    response.transient_status.as_ref(),
                ),
            }),
        }),
        (
            ExecutePhase::OutcomeUnknown,
            Some(FirmwareTerminalOutcome::PublishedWithoutAcknowledgement),
        ) => Ok(HttpSendResult {
            outcome: FirmwareSendOutcome::PublishedWithoutAcknowledgement,
            callback: None,
        }),
        _ => Err(SendError::Execute(anyhow!(
            "Hub firmware execute returned HTTP {status}: inconsistent successful Hub firmware execute response"
        ))),
    }
}

fn acknowledgement_matches(
    command: &StudioFirmwareCommand,
    acknowledgement: &FirmwareAcknowledgement,
) -> bool {
    acknowledgement.command == command.command_name()
        && acknowledgement.sequence_id == command.sequence_id()
}

async fn decode_success<T: DeserializeOwned>(
    response: reqwest::Response,
    context: &'static str,
) -> anyhow::Result<T> {
    let status = response.status();
    let bytes = response_bytes(response, context).await?;
    if !status.is_success() {
        bail!("Hub {context} returned HTTP {status}");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("decode Hub {context} response"))
}

async fn response_bytes(
    mut response: reqwest::Response,
    context: &'static str,
) -> anyhow::Result<Vec<u8>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_RESPONSE_BODY as u64),
        "Hub {context} response exceeded body limit"
    );
    let mut bytes = Vec::with_capacity(response.content_length().unwrap_or_default() as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(reqwest::Error::without_url)
        .with_context(|| format!("read Hub {context} response"))?
    {
        ensure!(
            chunk.len() <= MAX_RESPONSE_BODY - bytes.len(),
            "Hub {context} response exceeded body limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn firmware_url(hub_url: &str, printer_id: &str, suffix: &str) -> anyhow::Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(hub_url).context("parse Hub firmware URL")?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| anyhow!("Hub firmware URL cannot be a base URL"))?;
    segments.extend(["api", "v1", "plugin", "printers", printer_id]);
    segments.extend(suffix.split('/'));
    drop(segments);
    Ok(url)
}

fn write_diagnostic(writer: &mut impl Write, message: &str, error: &anyhow::Error) {
    let _ = writeln!(writer, "pandar network plugin {message}: {error:#}");
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn firmware_control_http_timeout_covers_agent_suback_and_ack_windows() {
        assert!(
            FIRMWARE_HTTP_TIMEOUT >= Duration::from_secs(7),
            "control timeout must cover a 5s SUBACK plus a 2s acknowledgement window"
        );
    }

    #[test]
    fn firmware_refresh_http_timeout_covers_three_agent_attempts() {
        assert!(
            FIRMWARE_REFRESH_HTTP_TIMEOUT >= Duration::from_secs(45),
            "refresh timeout must cover three 5s SUBACK plus 10s report attempts"
        );
    }
}
