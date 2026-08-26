use pandar_core::{CommandRecord, TenantId};

use super::{PrinterOperationRequest, TenantPrinterOperation, live};
use crate::{
    AppState,
    repositories::{AuditActor, PrintErrorAction, RepositoryError, WebPrintErrorRecovery},
    routes::ApiError,
};
use pandar_protocol::agent::v1::AgentCapability;

pub(in crate::routes) async fn dispatch_tenant_printer_operation(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    request: PrinterOperationRequest,
    actor: AuditActor,
) -> Result<CommandRecord, ApiError> {
    match request.into_tenant_operation()? {
        TenantPrinterOperation::Queued(operation) => {
            let command = state
                .commands()
                .enqueue_printer_operation_with_audit(tenant_id, printer_id, operation, actor)
                .await?;
            state.wake_agent(tenant_id, command.agent_id).await;
            Ok(command)
        }
        TenantPrinterOperation::HandlePrintError {
            error_action,
            error_generation,
        } => {
            dispatch(
                state,
                tenant_id,
                printer_id,
                error_action,
                error_generation,
                actor,
            )
            .await
        }
    }
}

async fn dispatch(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    error_action: PrintErrorAction,
    error_generation: u64,
    actor: AuditActor,
) -> Result<CommandRecord, ApiError> {
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
        .ok_or(RepositoryError::MissingPrinter)?;
    let agent_id = printer.agent_id;
    let _lease = state.sessions().transition_lease(agent_id).await;
    let capability = AgentCapability::HandlePrintErrorSequenceZeroPubackOnly;
    let token = state
        .sessions()
        .current_token_for_capability(tenant_id, agent_id, capability)
        .await
        .ok_or_else(live::printer_operation_unavailable)?;
    let persisted = state
        .commands()
        .create_web_print_error_sent_with_audit(
            tenant_id,
            printer_id,
            WebPrintErrorRecovery {
                action: error_action,
                error_generation,
                expected_agent_id: agent_id,
                expected_session_id: token.persisted_id(),
            },
            actor,
        )
        .await
        .map_err(web_recovery_error)?;
    debug_assert!(matches!(
        &persisted.operation,
        crate::repositories::PrinterOperationKind::HandlePrintError { sequence_id: 0, .. }
    ));
    if let Err(error) =
        live::dispatch_persisted_live_command(state, &persisted, token, capability).await
    {
        live::fail_live_dispatch(state, &persisted.command, error).await;
        return Err(live::printer_operation_unavailable());
    }

    Ok(persisted.command)
}

fn web_recovery_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::PrinterControlUnavailable
        | RepositoryError::AgentSessionNotCurrent
        | RepositoryError::MissingAgent
        | RepositoryError::CommandOwnershipMismatch => live::printer_operation_unavailable(),
        other => other.into(),
    }
}
