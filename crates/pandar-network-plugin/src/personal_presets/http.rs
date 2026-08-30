use std::time::Duration;

use anyhow::Context;
use reqwest::Method;
use serde::{Serialize, de::DeserializeOwned};

use super::model::{ErrorResponse, FullPreset, ListResponse, MutationResponse, PresetRequest};
use crate::{
    http::{hub_client, read_bounded_response_body, send_hub_request},
    normalize_hub_url,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct HttpFailure {
    pub(super) status: u32,
    pub(super) error: String,
    pub(super) code: Option<u8>,
    pub(super) cause: Option<anyhow::Error>,
}

impl HttpFailure {
    fn transport(error: anyhow::Error) -> Self {
        Self {
            status: 0,
            error: "hub_unavailable".into(),
            code: None,
            cause: Some(error),
        }
    }
}

pub(super) fn list(hub: &str, token: &str, bundle: &str) -> Result<ListResponse, HttpFailure> {
    let mut url = endpoint(hub, &["api", "v1", "plugin", "presets"])?;
    url.query_pairs_mut().append_pair("bundle_version", bundle);
    send::<ListResponse, ()>(Method::GET, url, token, None)
}

pub(super) fn get(hub: &str, token: &str, id: &str) -> Result<FullPreset, HttpFailure> {
    let url = endpoint(hub, &["api", "v1", "plugin", "presets", id])?;
    send::<FullPreset, ()>(Method::GET, url, token, None)
}

pub(super) fn create(
    hub: &str,
    token: &str,
    body: &PresetRequest,
) -> Result<MutationResponse, HttpFailure> {
    let url = endpoint(hub, &["api", "v1", "plugin", "presets"])?;
    send(Method::POST, url, token, Some(body))
}

pub(super) fn update(
    hub: &str,
    token: &str,
    id: &str,
    body: &PresetRequest,
) -> Result<MutationResponse, HttpFailure> {
    let url = endpoint(hub, &["api", "v1", "plugin", "presets", id])?;
    send(Method::PATCH, url, token, Some(body))
}

pub(super) fn delete(hub: &str, token: &str, id: &str) -> Result<(), HttpFailure> {
    let url = endpoint(hub, &["api", "v1", "plugin", "presets", id])?;
    send_empty(Method::DELETE, url, token)
}

fn endpoint(hub: &str, parts: &[&str]) -> Result<reqwest::Url, HttpFailure> {
    let hub = normalize_hub_url(hub.to_owned())
        .ok_or_else(|| HttpFailure::transport(anyhow::anyhow!("invalid Hub URL")))?;
    let mut url =
        reqwest::Url::parse(&hub).map_err(|error| HttpFailure::transport(error.into()))?;
    url.path_segments_mut()
        .map_err(|_| HttpFailure::transport(anyhow::anyhow!("Hub URL cannot be a base")))?
        .clear()
        .extend(parts);
    Ok(url)
}

fn send_empty(method: Method, url: reqwest::Url, token: &str) -> Result<(), HttpFailure> {
    crate::runtime().block_on(async {
        let response = send_hub_request(
            hub_client()
                .request(method, url)
                .bearer_auth(token)
                .timeout(HTTP_TIMEOUT),
            "personal preset HTTP request",
        )
        .await
        .map_err(HttpFailure::transport)?;
        let status = response.status();
        let text = read_bounded_response_body(response)
            .await
            .context("read personal preset HTTP response")
            .map_err(HttpFailure::transport)?;
        if status.is_success() {
            return Ok(());
        }
        let error = serde_json::from_str::<ErrorResponse>(&text).unwrap_or(ErrorResponse {
            error: "invalid_server_response".into(),
            code: None,
        });
        Err(HttpFailure {
            status: status.as_u16().into(),
            error: error.error,
            code: error.code,
            cause: None,
        })
    })
}

fn send<T: DeserializeOwned, B: Serialize>(
    method: Method,
    url: reqwest::Url,
    token: &str,
    body: Option<&B>,
) -> Result<T, HttpFailure> {
    crate::runtime().block_on(async {
        let request = hub_client()
            .request(method, url)
            .bearer_auth(token)
            .timeout(HTTP_TIMEOUT);
        let request = if let Some(body) = body {
            request.json(body)
        } else {
            request
        };
        let response = send_hub_request(request, "personal preset HTTP request")
            .await
            .map_err(HttpFailure::transport)?;
        let status = response.status();
        let text = read_bounded_response_body(response)
            .await
            .context("read personal preset HTTP response")
            .map_err(HttpFailure::transport)?;
        if status.is_success() {
            return serde_json::from_str(&text)
                .context("decode typed personal preset response")
                .map_err(HttpFailure::transport);
        }
        let error = serde_json::from_str::<ErrorResponse>(&text).unwrap_or(ErrorResponse {
            error: "invalid_server_response".into(),
            code: None,
        });
        Err(HttpFailure {
            status: status.as_u16().into(),
            error: error.error,
            code: error.code,
            cause: None,
        })
    })
}
