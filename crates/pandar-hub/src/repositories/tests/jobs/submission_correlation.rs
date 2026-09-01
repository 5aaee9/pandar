use super::*;
use crate::{
    db::Database,
    entities::jobs as job_entities,
    repositories::{
        AgentRepository, AuditActor, DuplicatePrintJob, JobWithArtifact, TenantRepository,
    },
};
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

const SUBMISSION_ID: &str = "2032858413";

#[tokio::test]
async fn print_report_correlates_by_persisted_bambu_submission_id() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    exercise_submission_id_correlation(database, tenants, agents, jobs).await;
}

pub(in crate::repositories::tests) async fn exercise_submission_id_correlation(
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    jobs: JobRepository,
) {
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "artifact-1",
            "Untitled.gcode.3mf",
        ))
        .await
        .unwrap();
    succeed_dispatch(
        &jobs,
        &created,
        tenant.id,
        agent.id,
        &printer_id,
        SUBMISSION_ID,
    )
    .await;

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            task_id: Some(SUBMISSION_ID.to_owned()),
            subtask_id: Some(SUBMISSION_ID.to_owned()),
            gcode_file: Some("/data/Metadata/plate_1.gcode".to_owned()),
            subtask_name: Some("Untitled".to_owned()),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();

    let job = applied.job.expect("submission id should correlate job").job;
    assert_eq!(job.id, created.job.id);
    assert_eq!(job.print.status, PrintStatus::Running);
}

#[tokio::test]
async fn successful_dispatch_persists_uploaded_url_for_projection() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "uploaded-url-artifact",
            "Untitled.gcode.3mf",
        ))
        .await
        .unwrap();
    succeed_dispatch(
        &jobs,
        &created,
        tenant.id,
        agent.id,
        &printer_id,
        SUBMISSION_ID,
    )
    .await;

    let projected = crate::job_projection::JobProjection::try_from(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let projected = serde_json::to_value(&projected).unwrap();
    assert_eq!(
        projected["command"]["uploaded_url"],
        serde_json::json!(format!("brtc://emmc/{}", created.artifact.filename))
    );
}

#[tokio::test]
async fn stale_submission_id_resumes_a_stalled_job() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "stalled-artifact",
            "stalled.gcode.3mf",
        ))
        .await
        .unwrap();
    succeed_dispatch(
        &jobs,
        &created,
        tenant.id,
        agent.id,
        &printer_id,
        SUBMISSION_ID,
    )
    .await;
    job_entities::Entity::update_many()
        .set(job_entities::ActiveModel {
            print_status: Set(PrintStatus::Stalled.as_str().to_owned()),
            created_at: Set("2026-06-20T00:00:00Z".to_owned()),
            ..Default::default()
        })
        .filter(job_entities::Column::Id.eq(created.job.id.to_string()))
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            task_id: Some(SUBMISSION_ID.to_owned()),
            subtask_id: Some(SUBMISSION_ID.to_owned()),
            gcode_file: Some("/data/Metadata/plate_99.gcode".to_owned()),
            subtask_name: Some("unmatched".to_owned()),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();

    let job = applied.job.expect("stalled submission should resume").job;
    assert_eq!(job.id, created.job.id);
    assert_eq!(job.print.status, PrintStatus::Running);
}

#[tokio::test]
async fn submission_id_selects_reprint_that_reuses_an_artifact() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let source = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "shared-artifact",
            "Untitled.gcode.3mf",
        ))
        .await
        .unwrap();
    let reprint = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            source.job.id,
            DuplicatePrintJob::default(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    succeed_dispatch(
        &jobs,
        &source,
        tenant.id,
        agent.id,
        &printer_id,
        "2032858412",
    )
    .await;
    succeed_dispatch(
        &jobs,
        &reprint,
        tenant.id,
        agent.id,
        &printer_id,
        SUBMISSION_ID,
    )
    .await;

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            task_id: Some(SUBMISSION_ID.to_owned()),
            subtask_id: Some(SUBMISSION_ID.to_owned()),
            gcode_file: Some("/data/Metadata/plate_1.gcode".to_owned()),
            subtask_name: Some("Untitled".to_owned()),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();

    assert_eq!(applied.job.unwrap().job.id, reprint.job.id);
    let source = jobs
        .get_for_tenant(tenant.id, source.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source.job.print.status, PrintStatus::Pending);
}

#[tokio::test]
async fn duplicate_submission_id_is_not_correlated() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let first = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "artifact-1",
            "first.gcode.3mf",
        ))
        .await
        .unwrap();
    let second = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "artifact-2",
            "second.gcode.3mf",
        ))
        .await
        .unwrap();
    for created in [&first, &second] {
        succeed_dispatch(
            &jobs,
            created,
            tenant.id,
            agent.id,
            &printer_id,
            SUBMISSION_ID,
        )
        .await;
    }

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            task_id: Some(SUBMISSION_ID.to_owned()),
            subtask_id: Some(SUBMISSION_ID.to_owned()),
            gcode_file: Some("/data/Metadata/plate_99.gcode".to_owned()),
            subtask_name: Some("unmatched".to_owned()),
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();

    assert!(applied.job.is_none());
    for job_id in [first.job.id, second.job.id] {
        let persisted = jobs
            .get_for_tenant(tenant.id, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.job.print.status, PrintStatus::Pending);
    }
}

async fn succeed_dispatch(
    jobs: &JobRepository,
    created: &JobWithArtifact,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    submission_id: &str,
) {
    jobs.mark_print_sent(created.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.mark_print_acknowledged(created.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.mark_print_succeeded_with_result(
        created.job.command_id,
        tenant_id,
        agent_id,
        Some(
            serde_json::to_string(&PrintProjectFileResultFixture {
                kind: "print_project_file",
                serial_number: format!("serial-{printer_id}"),
                job_id: created.job.id.to_string(),
                artifact_id: created.artifact.id.clone(),
                uploaded_path: created.artifact.filename.clone(),
                uploaded_url: format!("brtc://emmc/{}", created.artifact.filename),
                md5: "6F9A97E4F62BC8C6E83749A411BC34DA",
                mqtt: PrintProjectMqttResultFixture {
                    topic: format!("device/serial-{printer_id}/request"),
                    qos: 0,
                    payload: ProjectFileEnvelopeFixture {
                        print: ProjectFileIdentityFixture {
                            command: "project_file",
                            project_id: submission_id,
                            task_id: submission_id,
                            subtask_id: submission_id,
                            subtask_name: "Untitled",
                            file: &created.artifact.filename,
                            param: "Metadata/plate_1.gcode",
                        },
                    },
                },
            })
            .unwrap(),
        ),
    )
    .await
    .unwrap();
}

#[derive(Serialize)]
struct PrintProjectFileResultFixture<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    serial_number: String,
    job_id: String,
    artifact_id: String,
    uploaded_path: String,
    uploaded_url: String,
    md5: &'static str,
    mqtt: PrintProjectMqttResultFixture<'a>,
}

#[derive(Serialize)]
struct PrintProjectMqttResultFixture<'a> {
    topic: String,
    qos: u8,
    payload: ProjectFileEnvelopeFixture<'a>,
}

#[derive(Serialize)]
struct ProjectFileEnvelopeFixture<'a> {
    print: ProjectFileIdentityFixture<'a>,
}

#[derive(Serialize)]
struct ProjectFileIdentityFixture<'a> {
    command: &'static str,
    project_id: &'a str,
    task_id: &'a str,
    subtask_id: &'a str,
    subtask_name: &'static str,
    file: &'a str,
    param: &'static str,
}
