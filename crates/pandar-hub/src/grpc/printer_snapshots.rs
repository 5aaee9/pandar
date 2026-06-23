use pandar_core::{AgentId, TenantId};
use tonic::Status;

use crate::{
    AppState,
    printer_events::PrinterEvent,
    protocol::agent::v1::PrinterSnapshot,
    repositories::{PrinterSnapshotUpsert, RepositoryError},
};

pub async fn handle_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshot,
) -> Result<(), Status> {
    let serial_number = required(&snapshot.serial, "serial must not be blank")?;
    let name = required(&snapshot.name, "name must not be blank")?;
    let status = required(&snapshot.state, "state must not be blank")?;
    let model = trim_optional(snapshot.model);
    let observed_at = pandar_core::created_at_now();

    let printer = state
        .printers()
        .upsert_snapshot(
            tenant_id,
            agent_id,
            PrinterSnapshotUpsert {
                serial_number,
                name,
                model,
                status,
                observed_at,
            },
        )
        .await
        .map_err(repository_status)?;
    state
        .publish_printer_event(tenant_id, PrinterEvent::PrinterSnapshot { printer })
        .await;

    Ok(())
}

fn required(value: &str, message: &'static str) -> Result<String, Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(message));
    }

    Ok(value.to_string())
}

fn trim_optional(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn repository_status(err: RepositoryError) -> Status {
    match err {
        RepositoryError::MissingAgent => Status::not_found(err.to_string()),
        err => {
            tracing::error!(error = %format!("{err:#}"), "unexpected printer snapshot error");
            Status::internal("unexpected printer snapshot error")
        }
    }
}
