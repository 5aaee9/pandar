use pandar_core::{AgentId, TenantId};
use tonic::Status;

use crate::{
    AppState,
    printer_events::{PrinterEvent, printer_event_printer},
    protocol::agent::v1::PrinterMaterialsSnapshot,
    repositories::{CurrentMaterialPatchOutcome, MaterialPatchOutcome, RepositoryError},
    sessions::SessionToken,
};

pub async fn handle_materials_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterMaterialsSnapshot,
) -> Result<(), Status> {
    let serial = snapshot.serial.trim().to_owned();
    let printer_id = snapshot.printer_id.trim().to_owned();
    let printer_materials_json = snapshot.printer_materials_json;
    if serial.is_empty() {
        tracing::warn!(
            %tenant_id,
            %agent_id,
            printer_id = %printer_id,
            reason = "blank_serial",
            "ignored material snapshot event"
        );
        return Ok(());
    }
    if printer_materials_json.trim().is_empty() {
        tracing::warn!(
            %tenant_id,
            %agent_id,
            serial = %serial,
            printer_id = %printer_id,
            reason = "blank_materials",
            "ignored material snapshot event"
        );
        return Ok(());
    }
    if !printer_id.is_empty() && uuid::Uuid::parse_str(&printer_id).is_err() {
        tracing::warn!(
            %tenant_id,
            %agent_id,
            serial = %serial,
            printer_id = %printer_id,
            reason = "malformed_printer_id",
            "ignored material snapshot event"
        );
        return Ok(());
    }
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let applied = state
        .materials()
        .apply_snapshot_if_current(
            &token.persisted_id(),
            tenant_id,
            agent_id,
            &printer_id,
            serial.clone(),
            printer_materials_json,
        )
        .await;
    let applied = match applied {
        Ok(applied) => applied,
        Err(RepositoryError::AgentSessionNotCurrent) => return Ok(()),
        Err(err) => return Err(super::commands::repository_status(err)),
    };
    let (printer_id, outcome) = match applied {
        CurrentMaterialPatchOutcome::MissingPrinter => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "unknown_printer",
                "ignored material snapshot event"
            );
            return Ok(());
        }
        CurrentMaterialPatchOutcome::SerialMismatch {
            printer_id,
            printer_serial,
        } => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_serial = %printer_serial,
                printer_id = %printer_id,
                reason = "serial_mismatch",
                "ignored material snapshot event"
            );
            return Ok(());
        }
        CurrentMaterialPatchOutcome::Applied {
            printer_id,
            outcome,
        } => (printer_id, *outcome),
    };

    match outcome {
        MaterialPatchOutcome::Changed(_) => {}
        MaterialPatchOutcome::Invalid { error } => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "invalid_patch",
                error = %crate::redaction::redact_secrets(&error),
                "ignored material snapshot event"
            );
            return Ok(());
        }
        MaterialPatchOutcome::Empty => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "empty_patch",
                "ignored material snapshot event"
            );
            return Ok(());
        }
        MaterialPatchOutcome::Older => {
            tracing::info!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "older_patch",
                "ignored material snapshot event"
            );
            return Ok(());
        }
        MaterialPatchOutcome::Unchanged(_) => {
            tracing::info!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "unchanged_patch",
                "ignored material snapshot event"
            );
            return Ok(());
        }
    };

    let Some(printer) = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .map_err(super::commands::repository_status)?
    else {
        return Ok(());
    };
    let materials = state
        .materials()
        .latest_for_printer(tenant_id, &printer_id)
        .await
        .map_err(super::commands::repository_status)?;

    state
        .publish_printer_event(
            tenant_id,
            PrinterEvent::PrinterSnapshot {
                printer: Box::new(printer_event_printer(printer, materials)),
            },
        )
        .await;
    Ok(())
}
