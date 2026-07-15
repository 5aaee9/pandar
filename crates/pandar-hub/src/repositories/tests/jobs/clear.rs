use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};

use super::*;
use crate::{
    artifacts::{DEFAULT_MAX_ARTIFACT_BYTES, FilesystemArtifactStorage},
    entities::{
        audit_events, job_artifacts, job_filament_usages, jobs as job_entities, machine_events,
    },
    repositories::{
        AgentRepository, AuditActor, CommandRepository, JobRepository, TenantRepository,
    },
};

const STALE_AT: &str = "2000-01-01T00:00:00Z";

#[tokio::test]
async fn clear_jobs_removes_terminal_and_stalled_jobs_safely_on_sqlite() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let spool = tempfile::tempdir().unwrap();
    let storage = FilesystemArtifactStorage::new(spool.path(), DEFAULT_MAX_ARTIFACT_BYTES).unwrap();

    exercise_clear_jobs(database, tenants, agents, commands, jobs, &storage).await;
}

#[tokio::test]
async fn artifact_delete_failure_rolls_back_job_clear_on_sqlite() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants
        .create("clear-failure", "Clear Failure")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let terminal = create_job(&jobs, tenant.id, agent.id, &printer_id, "terminal").await;
    succeed(&jobs, terminal.job.command_id, tenant.id, agent.id).await;
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(terminal.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
    let storage = crate::repositories::tests::cleanup::storage::RecordingArtifactStorage::failing();

    let error = jobs
        .clear_for_tenant_with_audit(&storage, tenant.id, AuditActor::no_auth())
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains("delete failed"));
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(commands.count().await.unwrap(), 1);
    assert_eq!(artifact_count(&database, tenant.id).await, 1);
    assert_eq!(clear_audit_count(&database, tenant.id).await, 0);
}

pub(in crate::repositories::tests) async fn exercise_clear_jobs(
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    commands: CommandRepository,
    jobs: JobRepository,
    storage: &FilesystemArtifactStorage,
) {
    let tenant = tenants.create("clear-jobs", "Clear Jobs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let completed = create_job(&jobs, tenant.id, agent.id, &printer_id, "completed").await;
    succeed(&jobs, completed.job.command_id, tenant.id, agent.id).await;
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(completed.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
    insert_job_dependents(
        &database,
        tenant.id,
        agent.id,
        &printer_id,
        completed.job.id,
    )
    .await;

    let dispatch_failed =
        create_job(&jobs, tenant.id, agent.id, &printer_id, "dispatch-failed").await;
    jobs.mark_print_sent(dispatch_failed.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_failed(
        dispatch_failed.job.command_id,
        tenant.id,
        agent.id,
        "dispatch failed".to_owned(),
    )
    .await
    .unwrap();

    let queued = create_job(&jobs, tenant.id, agent.id, &printer_id, "queued").await;
    age_job(&database, queued.job.id, None).await;

    let waiting = create_job(&jobs, tenant.id, agent.id, &printer_id, "waiting").await;
    succeed(&jobs, waiting.job.command_id, tenant.id, agent.id).await;
    let fresh_print_update = pandar_core::created_at_now();
    age_job(&database, waiting.job.id, Some(&fresh_print_update)).await;

    let stalled = create_job(&jobs, tenant.id, agent.id, &printer_id, "stalled").await;
    succeed(&jobs, stalled.job.command_id, tenant.id, agent.id).await;
    age_job(&database, stalled.job.id, None).await;

    let running = create_job(&jobs, tenant.id, agent.id, &printer_id, "running").await;
    succeed(&jobs, running.job.command_id, tenant.id, agent.id).await;
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(running.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();
    age_job(&database, running.job.id, Some(STALE_AT)).await;

    let suspicious = create_job(&jobs, tenant.id, agent.id, &printer_id, "suspicious").await;
    jobs.mark_print_sent(suspicious.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_failed(
        suspicious.job.command_id,
        tenant.id,
        agent.id,
        "outcome unknown".to_owned(),
    )
    .await
    .unwrap();
    job_entities::Entity::update_many()
        .set(job_entities::ActiveModel {
            progress_percent: Set(Some(1)),
            ..Default::default()
        })
        .filter(job_entities::Column::Id.eq(suspicious.job.id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
    age_job(&database, suspicious.job.id, Some(STALE_AT)).await;

    let shared_active = create_job(&jobs, tenant.id, agent.id, &printer_id, "shared").await;
    let shared_terminal = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            shared_active.job.id,
            crate::repositories::DuplicatePrintJob {
                printer_id: None,
                plate_id: None,
                use_ams: None,
                flow_cali: None,
                timelapse: None,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    succeed(&jobs, shared_terminal.job.command_id, tenant.id, agent.id).await;
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(shared_terminal.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();

    let outcome = jobs
        .clear_for_tenant_with_audit(storage, tenant.id, AuditActor::no_auth())
        .await
        .unwrap();

    assert_eq!(outcome.deleted_jobs, 4);
    assert_eq!(outcome.retained_jobs, 5);
    assert_eq!(outcome.deleted_commands, 4);
    assert_eq!(outcome.deleted_artifacts, 3);
    assert_eq!(outcome.deleted_artifact_bytes, 126);
    assert!(
        jobs.get_for_tenant(tenant.id, completed.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, dispatch_failed.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, stalled.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, shared_terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    for retained in [queued, waiting, running, suspicious, shared_active.clone()] {
        assert!(
            jobs.get_for_tenant(tenant.id, retained.job.id)
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        job_artifacts::Entity::find_by_id(&shared_active.artifact.id)
            .one(&database.sea_orm_connection())
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(commands.count().await.unwrap(), 5);
    assert_eq!(job_count(&database, tenant.id).await, 5);
    assert_eq!(artifact_count(&database, tenant.id).await, 5);
    assert_eq!(clear_audit_count(&database, tenant.id).await, 1);
    assert_eq!(filament_usage_count(&database, tenant.id).await, 0);
    assert!(cleared_machine_event_is_printer_level(&database, completed.job.id).await);

    let replay = jobs
        .clear_for_tenant_with_audit(storage, tenant.id, AuditActor::no_auth())
        .await
        .unwrap();
    assert_eq!(replay.deleted_jobs, 0);
    assert_eq!(replay.retained_jobs, 5);
    assert_eq!(replay.deleted_commands, 0);
    assert_eq!(replay.deleted_artifacts, 0);
    assert_eq!(clear_audit_count(&database, tenant.id).await, 2);
}

async fn create_job(
    jobs: &JobRepository,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> crate::repositories::JobWithArtifact {
    jobs.create_print_job(create_input(tenant_id, agent_id, printer_id, artifact_id))
        .await
        .unwrap()
}

async fn succeed(
    jobs: &JobRepository,
    command_id: CommandId,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
) {
    jobs.mark_print_sent(command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.mark_print_succeeded(command_id, tenant_id, agent_id)
        .await
        .unwrap();
}

async fn age_job(database: &Database, job_id: JobId, print_updated_at: Option<&str>) {
    job_entities::Entity::update_many()
        .set(job_entities::ActiveModel {
            updated_at: Set(STALE_AT.to_owned()),
            print_updated_at: Set(print_updated_at.map(str::to_owned)),
            ..Default::default()
        })
        .filter(job_entities::Column::Id.eq(job_id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
}

async fn job_count(database: &Database, tenant_id: pandar_core::TenantId) -> u64 {
    job_entities::Entity::find()
        .filter(job_entities::Column::TenantId.eq(tenant_id.to_string()))
        .count(&database.sea_orm_connection())
        .await
        .unwrap()
}

async fn artifact_count(database: &Database, tenant_id: pandar_core::TenantId) -> u64 {
    job_artifacts::Entity::find()
        .filter(job_artifacts::Column::TenantId.eq(tenant_id.to_string()))
        .count(&database.sea_orm_connection())
        .await
        .unwrap()
}

async fn clear_audit_count(database: &Database, tenant_id: pandar_core::TenantId) -> u64 {
    audit_events::Entity::find()
        .filter(audit_events::Column::TenantId.eq(tenant_id.to_string()))
        .filter(audit_events::Column::Action.eq("job.clear"))
        .count(&database.sea_orm_connection())
        .await
        .unwrap()
}

async fn insert_job_dependents(
    database: &Database,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: JobId,
) {
    job_filament_usages::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id.to_string()),
        job_id: Set(job_id.to_string()),
        slot_index: Set(0),
        source: Set("ams_mapping".to_owned()),
        confidence: Set("mapped_no_quantity".to_owned()),
        created_at: Set(pandar_core::created_at_now()),
        ..Default::default()
    }
    .insert(&database.sea_orm_connection())
    .await
    .unwrap();
    machine_events::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id.to_string()),
        agent_id: Set(agent_id.to_string()),
        printer_id: Set(printer_id.to_owned()),
        job_id: Set(Some(job_id.to_string())),
        event_key: Set(format!("clear-job-{job_id}")),
        kind: Set("print_progress".to_owned()),
        severity: Set("info".to_owned()),
        message: Set("progress".to_owned()),
        code: Set(None),
        payload_json: Set("{}".to_owned()),
        observed_at: Set("2026-07-15T00:00:00Z".to_owned()),
        created_at: Set(pandar_core::created_at_now()),
    }
    .insert(&database.sea_orm_connection())
    .await
    .unwrap();
}

async fn filament_usage_count(database: &Database, tenant_id: pandar_core::TenantId) -> u64 {
    job_filament_usages::Entity::find()
        .filter(job_filament_usages::Column::TenantId.eq(tenant_id.to_string()))
        .count(&database.sea_orm_connection())
        .await
        .unwrap()
}

async fn cleared_machine_event_is_printer_level(database: &Database, job_id: JobId) -> bool {
    machine_events::Entity::find()
        .filter(machine_events::Column::EventKey.eq(format!("clear-job-{job_id}")))
        .filter(machine_events::Column::JobId.is_null())
        .count(&database.sea_orm_connection())
        .await
        .unwrap()
        == 1
}
