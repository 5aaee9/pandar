use pandar_core::TenantId;

use super::{FirmwareApiError, current_firmware_projection};
use crate::AppState;

pub(super) async fn require_prepared_token_for_path(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
    prepared_token: &str,
) -> Result<(), FirmwareApiError> {
    let identity = state.sessions().firmware_token_locator(prepared_token);
    if identity.as_ref().is_some_and(|identity| {
        identity.tenant_id != tenant_id || identity.printer_id != printer_id
    }) {
        return Err(FirmwareApiError::invalid_prepared_token());
    }

    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, printer_id)
        .await?
        .ok_or_else(FirmwareApiError::printer_not_found)?;
    let owns_printer =
        current_firmware_projection(state, tenant_id, printer.printer.agent_id, printer.firmware)
            .await?
            .is_some();
    match (owns_printer, identity) {
        (false, _) => Err(FirmwareApiError::unavailable()),
        (true, Some(_)) => Ok(()),
        (true, None) => Err(FirmwareApiError::invalid_prepared_token()),
    }
}
