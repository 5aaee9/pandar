mod device_features;
mod gcode_line;
pub(crate) mod live;
pub(crate) mod plate_mismatch;
mod request;
mod request_field;
mod web_recovery;

pub(super) use request::{PluginPrinterOperation, PrinterOperationRequest, TenantPrinterOperation};
pub(super) use web_recovery::dispatch_tenant_printer_operation;

use pandar_core::{CommandRecord, TenantId};

use crate::{
    AppState,
    repositories::{AuditActor, RepositoryError},
    routes::ApiError,
};

pub(super) async fn dispatch_plugin_printer_operation(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    request: PrinterOperationRequest,
    actor: AuditActor,
) -> Result<CommandRecord, ApiError> {
    match request.into_plugin_operation()? {
        PluginPrinterOperation::Queued(operation) => {
            let command = state
                .commands()
                .enqueue_printer_operation_with_audit(tenant_id, printer_id, operation, actor)
                .await
                .map_err(plugin_operation_error)?;
            state.wake_agent(command.tenant_id, command.agent_id).await;
            Ok(command)
        }
        PluginPrinterOperation::Live(operation) => {
            live::dispatch(state, tenant_id, printer_id, operation, actor).await
        }
    }
}

fn plugin_operation_error(error: RepositoryError) -> ApiError {
    match error {
        RepositoryError::PrinterControlUnavailable => live::printer_operation_unavailable(),
        other => other.into(),
    }
}

fn invalid_printer_control() -> ApiError {
    ApiError::bad_request("invalid_printer_control")
}
