use pandar_core::StudioSubmissionId;

use super::*;
use crate::repositories::{StudioTaskQuery, StudioTaskStatus};

#[path = "studio_tasks/snapshot.rs"]
mod snapshot;

fn printer_snapshot(serial_number: &str, name: &str) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number: serial_number.to_owned(),
        host: None,
        access_code: None,
        name: name.to_owned(),
        model: Some("X1C".to_owned()),
        status: Some("idle".to_owned()),
        observed_at: "2026-07-20T00:00:00Z".to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        cooling_system: None,
        nozzle_system: None,
        connection_authoritative: false,
        telemetry_authoritative: true,
    }
}

fn studio_query(status: StudioTaskStatus) -> StudioTaskQuery {
    StudioTaskQuery {
        printer_id: None,
        status: Some(status),
        offset: 0,
        limit: 20,
    }
}

#[tokio::test]
async fn studio_task_lookup_uses_stable_id_with_tenant_isolation() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants
        .create("studio-lookup-a", "Studio Lookup A")
        .await
        .unwrap();
    let other = tenants
        .create("studio-lookup-b", "Studio Lookup B")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let other_agent = agents.create(other.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let other_printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        &database,
        other.id,
        other_agent.id,
    )
    .await
    .unwrap();
    let created = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-lookup-a",
        ))
        .await
        .unwrap();
    let other_created = jobs
        .create_print_job(create_input(
            other.id,
            other_agent.id,
            &other_printer_id,
            "studio-lookup-b",
        ))
        .await
        .unwrap();
    let stable_id = StudioSubmissionId::try_from(1_i64).unwrap();

    let found = jobs
        .get_by_studio_submission_id(tenant.id, stable_id)
        .await
        .unwrap()
        .unwrap();
    let other_found = jobs
        .get_by_studio_submission_id(other.id, stable_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.job.id, created.job.id);
    assert_eq!(other_found.job.id, other_created.job.id);
    assert_ne!(found.job.id, other_found.job.id);
}

#[tokio::test]
async fn studio_device_serial_lookup_is_tenant_scoped() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants
        .create("studio-serial-a", "Studio Serial A")
        .await
        .unwrap();
    let other = tenants
        .create("studio-serial-b", "Studio Serial B")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let other_agent = agents.create(other.id, "agent").await.unwrap();
    let expected = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            printer_snapshot("STUDIO-SERIAL", "Tenant A Printer"),
        )
        .await
        .unwrap();
    let other_expected = printers
        .upsert_snapshot(
            other.id,
            other_agent.id,
            printer_snapshot("STUDIO-SERIAL", "Tenant B Printer"),
        )
        .await
        .unwrap();

    let found = printers
        .get_by_serial_for_tenant(tenant.id, "STUDIO-SERIAL")
        .await
        .unwrap()
        .unwrap();
    let other_found = printers
        .get_by_serial_for_tenant(other.id, "STUDIO-SERIAL")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, expected.id);
    assert_eq!(other_found.id, other_expected.id);
    assert_ne!(found.id, other_found.id);
}

#[tokio::test]
async fn studio_task_status_filters_follow_the_studio_projection() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants
        .create("studio-status", "Studio Status")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let in_progress = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-in-progress",
        ))
        .await
        .unwrap();
    let completed = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-completed",
        ))
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(completed.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();
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
    jobs.mark_for_command(
        completed.job.command_id,
        JobStatus::Failed,
        Some("dispatch marker after completion".to_owned()),
    )
    .await
    .unwrap();
    let job_failed = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-job-failed",
        ))
        .await
        .unwrap();
    jobs.mark_for_command(
        job_failed.job.command_id,
        JobStatus::Failed,
        Some("dispatch failed".to_owned()),
    )
    .await
    .unwrap();
    let print_failed = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-print-failed",
        ))
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(print_failed.job.id),
        None,
        "FAILED",
    ))
    .await
    .unwrap();
    let print_cancelled = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-print-cancelled",
        ))
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(print_cancelled.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(print_cancelled.job.id),
        None,
        "IDLE",
    ))
    .await
    .unwrap();

    let in_progress_page = jobs
        .list_studio_tasks(tenant.id, studio_query(StudioTaskStatus::InProgress))
        .await
        .unwrap();
    let completed_page = jobs
        .list_studio_tasks(tenant.id, studio_query(StudioTaskStatus::Completed))
        .await
        .unwrap();
    let failed_page = jobs
        .list_studio_tasks(tenant.id, studio_query(StudioTaskStatus::Failed))
        .await
        .unwrap();

    assert_eq!(
        in_progress_page
            .jobs
            .iter()
            .map(|job| job.job.id)
            .collect::<Vec<_>>(),
        vec![in_progress.job.id]
    );
    assert_eq!(
        completed_page
            .jobs
            .iter()
            .map(|job| job.job.id)
            .collect::<Vec<_>>(),
        vec![completed.job.id]
    );
    assert_eq!(completed_page.total, 1);
    let failed_ids = failed_page
        .jobs
        .iter()
        .map(|job| job.job.id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(failed_page.total, 3);
    assert_eq!(
        failed_ids,
        [
            job_failed.job.id,
            print_failed.job.id,
            print_cancelled.job.id,
        ]
        .into_iter()
        .collect()
    );
}

#[tokio::test]
async fn studio_task_pagination_counts_before_page_and_orders_newest_first() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("studio-page", "Studio Page").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let other_printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut created = Vec::new();
    for artifact_id in ["studio-oldest", "studio-middle", "studio-newest"] {
        created.push(
            jobs.create_print_job(create_input(tenant.id, agent.id, &printer_id, artifact_id))
                .await
                .unwrap(),
        );
    }
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    for (job, created_at) in [
        (&created[1], "2026-07-20T00:00:00.1Z"),
        (&created[2], "2026-07-20T00:00:00.11Z"),
    ] {
        sqlx::query("UPDATE jobs SET created_at = ?1 WHERE id = ?2")
            .bind(created_at)
            .bind(job.job.id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
    jobs.create_print_job(create_input(
        tenant.id,
        agent.id,
        &other_printer_id,
        "studio-other-printer",
    ))
    .await
    .unwrap();

    let page = jobs
        .list_studio_tasks(
            tenant.id,
            StudioTaskQuery {
                printer_id: Some(printer_id),
                status: None,
                offset: 1,
                limit: 1,
            },
        )
        .await
        .unwrap();

    assert_eq!(page.total, 3);
    assert_eq!(page.jobs.len(), 1);
    assert_eq!(page.jobs[0].job.id, created[1].job.id);
}
