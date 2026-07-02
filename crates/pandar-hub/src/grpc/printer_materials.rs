use pandar_core::{AgentId, Printer, TenantId};
use tonic::Status;

use crate::{
    AppState,
    printer_events::{PrinterEvent, printer_event_printer},
    protocol::agent::v1::PrinterMaterialsSnapshot,
    repositories::{MaterialPatchInput, MaterialPatchOutcome},
};

pub async fn handle_materials_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
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
    let Some(printer) = resolve_printer(state, tenant_id, agent_id, &serial, &printer_id).await?
    else {
        return Ok(());
    };
    if printer.serial_number != serial {
        tracing::warn!(
            %tenant_id,
            %agent_id,
            serial = %serial,
            printer_serial = %printer.serial_number,
            printer_id = %printer.id,
            reason = "serial_mismatch",
            "ignored material snapshot event"
        );
        return Ok(());
    }

    let outcome = state
        .materials()
        .upsert_from_patch_outcome(MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer.id.clone(),
            serial_number: serial.clone(),
            printer_materials_json,
        })
        .await
        .map_err(super::commands::repository_status)?;

    let materials = match outcome {
        MaterialPatchOutcome::Changed(materials) => materials,
        MaterialPatchOutcome::Invalid { error } => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer.id,
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
                printer_id = %printer.id,
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
                printer_id = %printer.id,
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
                printer_id = %printer.id,
                reason = "unchanged_patch",
                "ignored material snapshot event"
            );
            return Ok(());
        }
    };

    state
        .publish_printer_event(
            tenant_id,
            PrinterEvent::PrinterSnapshot {
                printer: Box::new(printer_event_printer(printer, Some(materials))),
            },
        )
        .await;
    Ok(())
}

async fn resolve_printer(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: &str,
    printer_id: &str,
) -> Result<Option<Printer>, Status> {
    if !printer_id.is_empty() {
        if uuid::Uuid::parse_str(printer_id).is_err() {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "malformed_printer_id",
                "ignored material snapshot event"
            );
            return Ok(None);
        }
        let Some(printer) = state
            .printers()
            .get_for_tenant(tenant_id, printer_id)
            .await
            .map_err(super::commands::repository_status)?
        else {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "unknown_printer",
                "ignored material snapshot event"
            );
            return Ok(None);
        };
        if printer.agent_id != agent_id {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %serial,
                printer_id = %printer_id,
                reason = "unknown_printer",
                "ignored material snapshot event"
            );
            return Ok(None);
        }
        return Ok(Some(printer));
    }

    let printer = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .map_err(super::commands::repository_status)?
        .into_iter()
        .find(|printer| printer.agent_id == agent_id && printer.serial_number == serial);
    if printer.is_none() {
        tracing::warn!(
            %tenant_id,
            %agent_id,
            serial = %serial,
            reason = "unknown_printer",
            "ignored material snapshot event"
        );
    }
    Ok(printer)
}
