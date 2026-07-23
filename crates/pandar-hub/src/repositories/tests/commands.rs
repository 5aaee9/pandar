use pandar_core::{AgentId, CommandId, CommandStatus};
use serde::Deserialize;

mod print_error;

use super::*;
use crate::repositories::{
    AuditActor, LinkPrinterPayload, PrintProjectFilePayload, PrinterOperationKind,
    PrinterOperationPayload, RefreshPrinterMaterialsPayload,
};

mod device_features_contract;
mod enqueue;
mod link;
mod printer_ops;

#[tokio::test]
async fn command_update_rejects_missing_command() {
    let (_, _, commands, tenant, agent) = command_repositories().await;

    let err = commands
        .mark_sent(CommandId::new(), tenant.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingCommand));
}

fn print_payload(printer_id: &str, serial_number: &str) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: "job-1".to_string(),
        artifact_id: "artifact-1".to_string(),
        printer_id: printer_id.to_string(),
        serial_number: serial_number.to_string(),
        filename: "plate.3mf".to_string(),
        storage_path: "tenant/artifact/plate.3mf".to_string(),
        artifact_download_path: "/api/v1/agents/agent-1/artifacts/artifact-1".to_string(),
        size_bytes: 3,
        plate_id: 1,
        use_ams: true,
        bed_leveling: false,
        auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
        flow_cali: false,
        auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
        auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
        timelapse: true,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
        studio_submission_id: crate::test_support::studio_submission_id_for_tests(),
        studio_metadata: Some(crate::test_support::studio_metadata_for_tests()),
    }
}

async fn set_command_updated_at(
    database: &crate::db::Database,
    command_id: CommandId,
    updated_at: &str,
) {
    match database {
        crate::db::Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET updated_at = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
        crate::db::Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET updated_at = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

fn link_payload(serial: &str) -> LinkPrinterPayload {
    LinkPrinterPayload {
        printer_type: "BambuLab".to_owned(),
        host: "192.0.2.10".to_owned(),
        access_code: format!("SECRET-{serial}"),
        name: Some("Office X1C".to_owned()),
    }
}

fn test_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "repository-test-token", vec!["*"])
}

fn test_audit_metadata() -> TestAuditActorMetadata {
    TestAuditActorMetadata {
        tenant_token_id: "repository-test-token".to_owned(),
        tenant_token_scopes: vec!["*".to_owned()],
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestAuditActorMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestPrintSpeedAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    speed_mode: u8,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestRefreshPrinterMaterialsAuditMetadata {
    agent_id: String,
    printer_id: String,
    serial_number: String,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestRedactedLinkPrinterPayload {
    printer_type: String,
    host: String,
    access_code: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestLinkPrinterAuditMetadata {
    printer_type: String,
    host: String,
    name: Option<String>,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
}

#[tokio::test]
async fn command_update_rejects_wrong_tenant() {
    let (tenants, _, commands, tenant, agent) = command_repositories().await;
    let other = tenants.create("beta", "Beta Labs").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, other.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_update_rejects_wrong_agent() {
    let (_, agents, commands, tenant, agent) = command_repositories().await;
    let other = agents.create(tenant.id, "other").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, tenant.id, other.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_sent_ack_success_flow() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let sent = commands
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(sent.status, CommandStatus::Sent);
    let acked = commands
        .mark_acknowledged(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(acked.status, CommandStatus::Acknowledged);
    let succeeded = commands
        .mark_succeeded(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(succeeded.status, CommandStatus::Succeeded);
}

#[tokio::test]
async fn command_ack_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "rejected")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("rejected"));
}

#[tokio::test]
async fn command_result_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_acknowledged(command_id, tenant.id, agent.id)
        .await
        .unwrap();

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "printer unavailable")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("printer unavailable"));
}
