use std::{future::Future, net::Ipv4Addr};

use axum::http::StatusCode;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};

use crate::{
    protocol::agent::v1::{HubCommand, LinkPrinter, hub_command},
    repositories::{LinkPrinterPayload, RepositoryResult},
    routes::ApiError,
};

pub(super) fn parse_agent_id(value: &str) -> Result<AgentId, ApiError> {
    AgentId::parse(value).map_err(|_| ApiError::bad_request("invalid_agent_id"))
}

pub(super) fn parse_command_id(value: &str) -> Result<CommandId, ApiError> {
    CommandId::parse(value).map_err(|_| ApiError::bad_request("invalid_command_id"))
}

pub(super) fn parse_printer_id(value: &str) -> Result<&str, ApiError> {
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid_printer_id"))?;
    Ok(value)
}

impl super::LinkPrinterRequest {
    pub(super) fn into_payload(self) -> Result<LinkPrinterPayload, ApiError> {
        let printer_type = trim_required(self.printer_type)?;
        if printer_type != "BambuLab" {
            return Err(ApiError::bad_request("bad_request"));
        }

        let host = trim_required(self.host)?;
        parse_lan_host(&host)?;

        Ok(LinkPrinterPayload {
            printer_type,
            host,
            access_code: trim_required(self.access_code)?,
            name: trim_optional(self.name),
        })
    }
}

impl super::UpdatePrinterRequest {
    pub(super) fn into_fields(
        self,
        existing_host: Option<String>,
        existing_access_code: Option<String>,
    ) -> Result<(String, String, String), ApiError> {
        let requested_host = trim_optional(Some(self.host));
        let host = requested_host
            .clone()
            .or_else(|| existing_host.clone())
            .ok_or_else(|| ApiError::bad_request("bad_request"))?;
        parse_lan_host(&host)?;
        let requested_access_code = trim_optional(Some(self.access_code));
        if requested_host.is_some()
            && existing_host.as_deref() != Some(host.as_str())
            && requested_access_code.is_none()
        {
            return Err(ApiError::bad_request(
                "access_code_required_for_host_change",
            ));
        }
        let access_code = requested_access_code
            .or(existing_access_code)
            .ok_or_else(|| ApiError::bad_request("bad_request"))?;

        Ok((trim_required(self.name)?, host, access_code))
    }
}

fn parse_lan_host(value: &str) -> Result<Ipv4Addr, ApiError> {
    let address = value
        .parse::<Ipv4Addr>()
        .map_err(|_| ApiError::bad_request("bad_request"))?;
    if !(address.is_private() || address.is_link_local()) {
        return Err(ApiError::bad_request("printer_host_must_be_private"));
    }
    Ok(address)
}

pub(super) fn trim_required(value: String) -> Result<String, ApiError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    Ok(value)
}

pub(super) fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn link_printer_hub_command(
    command_id: CommandId,
    payload: &LinkPrinterPayload,
) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: payload.host.clone(),
            access_code: payload.access_code.clone(),
            name: payload.name.clone().unwrap_or_default(),
            printer_type: payload.printer_type.clone(),
        })),
    }
}

pub(super) async fn fail_link_printer_dispatch_after_commit<F, Fut>(
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: &LinkPrinterPayload,
    error: String,
    mark_failed: F,
) -> Result<CommandRecord, ApiError>
where
    F: FnOnce(CommandId, TenantId, AgentId, String) -> Fut,
    Fut: Future<Output = RepositoryResult<CommandRecord>>,
{
    mark_failed(command_id, tenant_id, agent_id, error)
        .await
        .map_err(|err| {
            let error = crate::redaction::redact_link_printer_secret(
                &format!("{err:#}"),
                &payload.access_code,
            );
            tracing::error!(
                command_id = %command_id,
                error = %error,
                "failed to mark live printer link dispatch failed after command commit"
            );
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
}
