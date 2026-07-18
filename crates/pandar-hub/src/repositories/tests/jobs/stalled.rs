use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use super::*;
use crate::{
    db::Database,
    entities::{commands as command_entities, jobs as job_entities},
    repositories::{AgentRepository, AuditActor, JobRepository, TenantRepository},
};

const WAITING_SINCE: &str = "2026-07-17T00:00:00.000Z";
const AT_THRESHOLD: &str = "2026-07-17T00:15:00Z";
const OVER_THRESHOLD: &str = "2026-07-17T00:15:01Z";

#[test]
fn stalled_print_status_migrations_cover_both_backends() {
    for migration in [
        include_str!("../../../../migrations/sqlite/20260717000000_stalled_print_jobs.sql"),
        include_str!("../../../../migrations/postgres/20260717000000_stalled_print_jobs.sql"),
    ] {
        assert!(migration.contains("'pending', 'stalled', 'running'"));
        assert!(migration.contains("UPDATE jobs SET print_status_v2 = print_status"));
        assert!(migration.contains("RENAME COLUMN print_status_v2 TO print_status"));
    }
}

#[tokio::test]
async fn pending_print_jobs_become_recoverable_stalled_jobs_on_sqlite() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    exercise_stalled_print_jobs(database, tenants, agents, jobs).await;
}

pub(in crate::repositories::tests) async fn exercise_stalled_print_jobs(
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    jobs: JobRepository,
) {
    let tenant = tenants
        .create("stalled-print-jobs", "Stalled Print Jobs")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let candidate = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "stalled-candidate",
            "candidate.3mf",
        ))
        .await
        .unwrap();
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        candidate.job.command_id,
        WAITING_SINCE,
    )
    .await;
    set_waiting_state(&database, candidate.job.id, None, None, None, None).await;
    jobs.apply_print_report(ApplyPrintReport {
        gcode_state: None,
        percent: Some(0),
        current_layer: Some(0),
        observed_at: "2026-07-17T00:14:59Z".to_owned(),
        ..report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(candidate.job.id),
            None,
            "PREPARE",
        )
    })
    .await
    .unwrap();
    let candidate_before = jobs
        .get_for_tenant(tenant.id, candidate.job.id)
        .await
        .unwrap()
        .unwrap();

    let recent_dispatch = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "fresh"))
        .await
        .unwrap();
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        recent_dispatch.job.command_id,
        "2026-07-17T00:14:59Z",
    )
    .await;
    set_waiting_state(&database, recent_dispatch.job.id, None, None, None, None).await;

    let progressed = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "progressed"))
        .await
        .unwrap();
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        progressed.job.command_id,
        WAITING_SINCE,
    )
    .await;
    set_waiting_state(&database, progressed.job.id, None, Some(1), None, None).await;

    let started = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "started"))
        .await
        .unwrap();
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        started.job.command_id,
        WAITING_SINCE,
    )
    .await;
    set_waiting_state(
        &database,
        started.job.id,
        None,
        None,
        None,
        Some(WAITING_SINCE),
    )
    .await;

    let dispatch_failed = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "dispatch-failed",
        ))
        .await
        .unwrap();
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
    set_waiting_state(&database, dispatch_failed.job.id, None, None, None, None).await;

    let queued = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "queued"))
        .await
        .unwrap();
    set_waiting_state(&database, queued.job.id, None, None, None, None).await;

    assert!(
        jobs.mark_stalled_pending_jobs(AT_THRESHOLD)
            .await
            .unwrap()
            .is_empty()
    );
    let stalled = jobs
        .mark_stalled_pending_jobs(OVER_THRESHOLD)
        .await
        .unwrap();
    assert_eq!(stalled.len(), 1);
    assert_eq!(stalled[0].job.id, candidate.job.id);
    assert_eq!(stalled[0].job.print.status, PrintStatus::Stalled);
    assert_eq!(stalled[0].job.updated_at, candidate_before.job.updated_at);
    assert_eq!(
        stalled[0].job.print.updated_at.as_deref(),
        Some("2026-07-17T00:14:59Z")
    );
    assert!(
        jobs.mark_stalled_pending_jobs(OVER_THRESHOLD)
            .await
            .unwrap()
            .is_empty()
    );

    for guarded in [
        recent_dispatch,
        progressed,
        started,
        dispatch_failed,
        queued,
    ] {
        assert_eq!(
            jobs.get_for_tenant(tenant.id, guarded.job.id)
                .await
                .unwrap()
                .unwrap()
                .job
                .print
                .status,
            PrintStatus::Pending,
        );
    }

    let resumed = jobs
        .apply_print_report(ApplyPrintReport {
            gcode_state: None,
            percent: Some(1),
            current_layer: Some(1),
            job_id: None,
            task_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("/cache/candidate.3mf".to_owned()),
            observed_at: "2026-07-17T00:16:00Z".to_owned(),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap()
        .job
        .unwrap()
        .job;
    assert_eq!(resumed.id, candidate.job.id);
    assert_eq!(resumed.print.status, PrintStatus::Running);
    assert_eq!(
        resumed.print.started_at.as_deref(),
        Some("2026-07-17T00:16:00Z")
    );

    let reprintable = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "stalled-reprint",
            "reprint.3mf",
        ))
        .await
        .unwrap();
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        reprintable.job.command_id,
        WAITING_SINCE,
    )
    .await;
    set_waiting_state(&database, reprintable.job.id, None, None, None, None).await;
    assert_eq!(
        jobs.mark_stalled_pending_jobs("2026-07-17T00:16:00Z")
            .await
            .unwrap()
            .len(),
        1,
    );
    let reprint = jobs
        .reprint_with_audit(
            tenant.id,
            reprintable.job.id,
            crate::repositories::DuplicatePrintJob::default(),
            None,
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    assert_eq!(reprint.job.status, JobStatus::Queued);
    assert_eq!(reprint.job.print.status, PrintStatus::Pending);
    succeed_job_at(
        &database,
        &jobs,
        tenant.id,
        agent.id,
        reprint.job.command_id,
        "2026-07-17T00:16:30Z",
    )
    .await;
    let reprint_started = jobs
        .apply_print_report(ApplyPrintReport {
            job_id: None,
            task_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("/cache/reprint.3mf".to_owned()),
            observed_at: "2026-07-17T00:16:31Z".to_owned(),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap()
        .job
        .unwrap()
        .job;
    assert_eq!(reprint_started.id, reprint.job.id);
    assert_eq!(reprint_started.print.status, PrintStatus::Running);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, reprintable.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .print
            .status,
        PrintStatus::Stalled,
    );

    let connection = database.sea_orm_connection();
    let invalid = job_entities::Entity::update_many()
        .set(job_entities::ActiveModel {
            print_status: Set("unknown".to_owned()),
            ..Default::default()
        })
        .filter(job_entities::Column::Id.eq(reprintable.job.id.to_string()))
        .exec(&connection)
        .await;
    assert!(invalid.is_err());
}

async fn succeed_job_at(
    database: &Database,
    jobs: &JobRepository,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    updated_at: &str,
) {
    jobs.mark_print_sent(command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.mark_print_succeeded(command_id, tenant_id, agent_id)
        .await
        .unwrap();
    command_entities::Entity::update_many()
        .set(command_entities::ActiveModel {
            updated_at: Set(updated_at.to_owned()),
            ..Default::default()
        })
        .filter(command_entities::Column::Id.eq(command_id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
}

async fn set_waiting_state(
    database: &Database,
    job_id: JobId,
    print_updated_at: Option<&str>,
    progress_percent: Option<i32>,
    current_layer: Option<i32>,
    print_started_at: Option<&str>,
) {
    job_entities::Entity::update_many()
        .set(job_entities::ActiveModel {
            updated_at: Set(WAITING_SINCE.to_owned()),
            print_updated_at: Set(print_updated_at.map(str::to_owned)),
            progress_percent: Set(progress_percent),
            current_layer: Set(current_layer),
            print_started_at: Set(print_started_at.map(str::to_owned)),
            ..Default::default()
        })
        .filter(job_entities::Column::Id.eq(job_id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
}
