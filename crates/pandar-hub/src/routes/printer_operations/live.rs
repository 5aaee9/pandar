use pandar_core::{CommandRecord, TenantId};

use super::plugin_operation_error;
use crate::{
    AppState,
    grpc::commands::live_printer_operation_hub_command,
    protocol::agent::v1::AgentCapability,
    repositories::{AuditActor, PrinterOperationKind, RepositoryError},
    routes::ApiError,
    sessions::LiveDispatchError,
};

pub(super) async fn dispatch(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    operation: PrinterOperationKind,
    actor: AuditActor,
) -> Result<CommandRecord, ApiError> {
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
        .ok_or(RepositoryError::MissingPrinter)?;
    let capability = AgentCapability::HandlePrintError;
    let Some(token) = state
        .sessions()
        .current_token_for_capability(tenant_id, printer.agent_id, capability)
        .await
    else {
        return Err(printer_operation_unavailable());
    };
    let command = state
        .commands()
        .create_printer_operation_sent_with_audit(
            tenant_id,
            printer_id,
            printer.agent_id,
            operation.clone(),
            actor,
        )
        .await
        .map_err(plugin_operation_error)?;
    let hub_command =
        live_printer_operation_hub_command(command.id, printer.serial_number, operation);
    if let Err(error) = state
        .sessions()
        .try_dispatch_live_command_with_capability(
            tenant_id,
            command.agent_id,
            token,
            capability,
            command.id,
            hub_command,
        )
        .await
    {
        fail_live_dispatch(state, &command, error).await;
        return Err(printer_operation_unavailable());
    }

    Ok(command)
}

async fn fail_live_dispatch(
    state: &AppState,
    command: &CommandRecord,
    dispatch_error: LiveDispatchError,
) {
    let failure = format!("live printer operation dispatch failed: {dispatch_error:?}");
    if let Err(error) = state
        .commands()
        .mark_failed(command.id, command.tenant_id, command.agent_id, failure)
        .await
    {
        tracing::error!(
            command_id = %command.id,
            ?dispatch_error,
            error = %format!("{error:#}"),
            "failed to record live printer operation dispatch failure"
        );
    }
}

pub(super) fn printer_operation_unavailable() -> ApiError {
    ApiError::bad_request("printer_operation_unavailable")
}
