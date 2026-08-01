use pandar_core::{CommandRecord, TenantId};

use super::plugin_operation_error;
use crate::{
    AppState,
    grpc::commands::live_printer_operation_hub_command,
    protocol::agent::v1::AgentCapability,
    repositories::{
        AuditActor, PersistedLivePrinterOperation, PrinterOperationKind, RepositoryError,
    },
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
    dispatch_with_capability(
        state,
        tenant_id,
        printer_id,
        operation,
        actor,
        AgentCapability::HandlePrintError,
    )
    .await
}

pub(crate) async fn dispatch_with_capability(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    operation: PrinterOperationKind,
    actor: AuditActor,
    capability: AgentCapability,
) -> Result<CommandRecord, ApiError> {
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
        .ok_or(RepositoryError::MissingPrinter)?;
    let Some(token) = state
        .sessions()
        .current_token_for_capability(tenant_id, printer.agent_id, capability)
        .await
    else {
        return Err(printer_operation_unavailable());
    };
    dispatch_for_printer_with_token(
        state, tenant_id, printer, operation, actor, token, capability,
    )
    .await
}

pub(crate) async fn dispatch_for_printer_with_token(
    state: &AppState,
    tenant_id: TenantId,
    printer: pandar_core::Printer,
    operation: PrinterOperationKind,
    actor: AuditActor,
    token: crate::sessions::SessionToken,
    capability: AgentCapability,
) -> Result<CommandRecord, ApiError> {
    let command = state
        .commands()
        .create_printer_operation_sent_with_audit(
            tenant_id,
            &printer.id,
            printer.agent_id,
            operation.clone(),
            actor,
        )
        .await
        .map_err(plugin_operation_error)?;
    let persisted = PersistedLivePrinterOperation {
        command,
        serial_number: printer.serial_number,
        operation,
    };
    if let Err(error) = dispatch_persisted_live_command(state, &persisted, token, capability).await
    {
        fail_live_dispatch(state, &persisted.command, error).await;
        return Err(printer_operation_unavailable());
    }

    Ok(persisted.command)
}

pub(super) async fn dispatch_persisted_live_command(
    state: &AppState,
    persisted: &PersistedLivePrinterOperation,
    token: crate::sessions::SessionToken,
    capability: AgentCapability,
) -> Result<(), LiveDispatchError> {
    let hub_command = live_printer_operation_hub_command(
        persisted.command.id,
        persisted.serial_number.clone(),
        persisted.operation.clone(),
    );
    state
        .sessions()
        .try_dispatch_live_command_with_capability(
            persisted.command.tenant_id,
            persisted.command.agent_id,
            token,
            capability,
            persisted.command.id,
            hub_command,
        )
        .await
}

pub(super) async fn fail_live_dispatch(
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
