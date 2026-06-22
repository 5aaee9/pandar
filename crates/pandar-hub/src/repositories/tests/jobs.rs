use pandar_core::{AgentId, CommandId, JobId, JobStatus};
use serde_json::Value;

use super::*;
use crate::repositories::CreatePrintJob;

#[tokio::test]
async fn job_repository_create_print_job_links_artifact_command_and_job() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();

    assert_eq!(created.artifact.id, "artifact-1");
    assert_eq!(created.job.printer_id, printer_id);
    assert_eq!(created.job.status, JobStatus::Queued);
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn print_command_created_by_job_transaction_has_linked_job() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    let payload: Value = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "print_project_file");
    assert_eq!(command.id, created.job.command_id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload["job_id"], created.job.id.to_string());
    assert_eq!(payload["artifact_id"], created.artifact.id);
    assert_eq!(payload["printer_id"], printer_id);
    assert!(
        payload["serial_number"]
            .as_str()
            .unwrap()
            .starts_with("serial-")
    );
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .command_id,
        command.id
    );
}

#[tokio::test]
async fn job_repository_list_returns_newest_first() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let first = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let second = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-2"))
        .await
        .unwrap();

    let listed = jobs.list_for_tenant(tenant.id).await.unwrap();

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].job.id, second.job.id);
    assert_eq!(listed[1].job.id, first.job.id);
}

#[tokio::test]
async fn job_repository_get_returns_none_for_unknown_job() {
    let (_, tenants, _, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    assert_eq!(
        jobs.get_for_tenant(tenant.id, JobId::new()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn job_repository_rejects_missing_tenant_on_list() {
    let (_, _, _, _, _, jobs) = repositories().await;

    let err = jobs
        .list_for_tenant(pandar_core::TenantId::new())
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingTenant));
}

#[tokio::test]
async fn job_repository_rejects_wrong_tenant_printer() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, acme.id, agent.id)
            .await
            .unwrap();

    let err = jobs
        .create_print_job(create_input(beta.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}

#[tokio::test]
async fn job_repository_mark_for_command_tracks_ack_success_failure() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();

    let acked = jobs
        .mark_for_command(created.job.command_id, JobStatus::Acknowledged, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(acked.status, JobStatus::Acknowledged);
    let succeeded = jobs
        .mark_for_command(created.job.command_id, JobStatus::Succeeded, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(succeeded.status, JobStatus::Succeeded);
    let duplicate = jobs
        .mark_for_command(
            created.job.command_id,
            JobStatus::Failed,
            Some("late".to_string()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(duplicate.status, JobStatus::Succeeded);
    assert_eq!(
        jobs.mark_for_command(
            CommandId::new(),
            JobStatus::Failed,
            Some("missing".to_string())
        )
        .await
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn job_repository_print_command_transitions_update_command_and_job_together() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();

    let sent = jobs
        .mark_print_sent(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(sent.status, pandar_core::CommandStatus::Sent);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Sent
    );

    let acked = jobs
        .mark_print_acknowledged(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(acked.status, pandar_core::CommandStatus::Acknowledged);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Acknowledged
    );

    let succeeded = jobs
        .mark_print_succeeded(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(succeeded.status, pandar_core::CommandStatus::Succeeded);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Succeeded
    );
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn invalid_persisted_job_status_is_reported() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE jobs SET status = 'printing' WHERE id = ?1")
        .bind(created.job.id.to_string())
        .execute(pool)
        .await
        .unwrap();

    let err = jobs.list_for_tenant(tenant.id).await.unwrap_err();

    assert!(
        matches!(err, RepositoryError::InvalidPersistedJobStatus(status) if status == "printing")
    );
}

#[tokio::test]
async fn job_repository_create_rolls_back_command_when_job_insert_fails() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let err = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, ""))
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::Database(_)));
    assert_eq!(commands.count().await.unwrap(), 0);
}

fn create_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_string(),
        agent_id,
        artifact_id: artifact_id.to_string(),
        artifact_filename: "plate.3mf".to_string(),
        artifact_content_type: "model/3mf".to_string(),
        artifact_size_bytes: 42,
        artifact_storage_path: format!("{tenant_id}/{artifact_id}/plate.3mf"),
        plate_id: 1,
        use_ams: true,
        flow_cali: false,
        timelapse: false,
    }
}
