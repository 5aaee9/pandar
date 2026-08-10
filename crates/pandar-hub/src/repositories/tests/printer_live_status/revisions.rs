use super::*;
use crate::repositories::{ApplyPrintReport, AuditActor};

fn report(tenant_id: TenantId, agent_id: AgentId, serial: String) -> ApplyPrintReport {
    ApplyPrintReport {
        tenant_id,
        agent_id,
        serial,
        task_id: None,
        job_id: None,
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: None,
        percent: None,
        speed_level: None,
        remaining_time_minutes: None,
        current_layer: None,
        total_layers: None,
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "1999-01-01T00:00:00Z".to_owned(),
    }
}

pub(crate) async fn exercise_atomic_revisions(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("printer-revisions", "Printer Revisions")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let serial = format!("serial-{printer_id}");

    let initial = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(initial.state_revision, 1);

    let last_seen = jobs
        .apply_print_report(report(tenant.id, agent.id, serial.clone()))
        .await
        .unwrap();
    assert_eq!(last_seen.printer_id.as_deref(), Some(printer_id.as_str()));
    assert!(!last_seen.live_status_changed);
    assert_eq!(last_seen.printer.as_ref().unwrap().state_revision, 2);

    printers
        .update_details_with_audit(
            tenant.id,
            &printer_id,
            "Revision Printer".to_owned(),
            "192.0.2.10".to_owned(),
            "access-code".to_owned(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    assert_eq!(current_revision(&printers, tenant.id).await, 3);

    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: serial.clone(),
                host: None,
                access_code: None,
                name: "Ignored Snapshot Name".to_owned(),
                model: Some("A1".to_owned()),
                status: Some("RUNNING".to_owned()),
                observed_at: "2026-07-10T12:01:00Z".to_owned(),
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
            },
        )
        .await
        .unwrap();
    assert_eq!(current_revision(&printers, tenant.id).await, 4);

    let mut live_report = report(tenant.id, agent.id, serial);
    live_report.percent = Some(1);
    live_report.gcode_state = Some("RUNNING".to_owned());
    let live = jobs.apply_print_report(live_report).await.unwrap();
    assert!(live.live_status_changed);
    assert_eq!(live.printer.as_ref().unwrap().state_revision, 5);
}

pub(crate) async fn exercise_concurrent_revision_writers(database: Database, slug: &str) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants.create(slug, "Concurrent Revisions").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let serial = format!("serial-{printer_id}");
    let first = JobRepository::new(database.clone());
    let second = JobRepository::new(database);

    let (first, second) = tokio::join!(
        first.apply_print_report(report(tenant.id, agent.id, serial.clone())),
        second.apply_print_report(report(tenant.id, agent.id, serial)),
    );
    first.unwrap();
    second.unwrap();

    assert_eq!(current_revision(&printers, tenant.id).await, 3);
}

async fn current_revision(printers: &PrinterRepository, tenant_id: TenantId) -> u64 {
    printers
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .state_revision
}

#[tokio::test]
async fn sqlite_non_material_mutations_increment_revision_once() {
    exercise_atomic_revisions(sqlite_database().await).await;
}

#[tokio::test]
async fn sqlite_concurrent_print_report_writers_do_not_lose_a_revision() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        temp_dir.path().join("revision-race.sqlite").display()
    );
    let database = Database::connect(&DatabaseConfig::from_url(database_url).unwrap())
        .await
        .unwrap();
    database.migrate().await.unwrap();
    exercise_concurrent_revision_writers(database, "sqlite-revision-race").await;
}

#[tokio::test]
async fn positive_error_marker_uses_hub_receive_time_not_agent_observed_at() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("receive-time", "Receive Time")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = report(tenant.id, agent.id, format!("serial-{printer_id}"));
    input.task_id = Some("task".to_owned());
    input.gcode_state = Some("RUNNING".to_owned());
    input.print_error = Some(83_918_929);

    jobs.apply_print_report(input).await.unwrap();

    let marker = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .live_status
        .error_received_at
        .unwrap();
    assert_ne!(marker, "1999-01-01T00:00:00Z");
    time::OffsetDateTime::parse(&marker, &time::format_description::well_known::Rfc3339).unwrap();
}
