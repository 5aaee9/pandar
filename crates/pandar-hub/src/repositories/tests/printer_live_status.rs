use super::*;
use crate::repositories::{ApplyPrintReport, PrinterHms};

pub(super) async fn exercise_printer_live_status(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("printer-live-status", "Printer Live Status")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let serial = format!("serial-{printer_id}");

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: serial.clone(),
            task_id: Some("external-task-42".to_string()),
            job_id: None,
            artifact_id: None,
            subtask_id: Some("external-subtask-7".to_string()),
            gcode_file: Some("external.3mf".to_string()),
            subtask_name: Some("External plate".to_string()),
            gcode_state: Some("RUNNING".to_string()),
            percent: Some(42),
            remaining_time_minutes: Some(87),
            current_layer: Some(12),
            total_layers: Some(120),
            hms: Some(vec![PrinterHms {
                attr: 0x0102_0304,
                code: 0x0506_0708,
            }]),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-09T10:00:00Z".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(applied.job, None);
    let persisted = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(persisted.printer.status, "offline");
    assert_eq!(persisted.printer.last_seen_at, "2026-07-09T10:00:00Z");
    assert_eq!(
        persisted.live_status.gcode_state.as_deref(),
        Some("RUNNING")
    );
    assert_eq!(
        persisted.live_status.task_id.as_deref(),
        Some("external-task-42")
    );
    assert_eq!(
        persisted.live_status.subtask_id.as_deref(),
        Some("external-subtask-7")
    );
    assert_eq!(persisted.live_status.progress_percent, Some(42));
    assert_eq!(persisted.live_status.remaining_time_minutes, Some(87));
    assert_eq!(persisted.live_status.current_layer, Some(12));
    assert_eq!(persisted.live_status.total_layers, Some(120));
    assert_eq!(
        persisted.live_status.gcode_file.as_deref(),
        Some("external.3mf")
    );
    assert_eq!(
        persisted.live_status.subtask_name.as_deref(),
        Some("External plate")
    );
    assert_eq!(
        persisted.live_status.hms,
        vec![PrinterHms {
            attr: 0x0102_0304,
            code: 0x0506_0708,
        }]
    );

    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: serial.clone(),
                host: None,
                access_code: None,
                name: "Fixture Printer".to_string(),
                model: None,
                status: "unknown".to_string(),
                observed_at: "2026-07-09T10:00:30Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: Some("42".to_string()),
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            },
        )
        .await
        .unwrap();
    let after_snapshot = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(after_snapshot.printer.status, "unknown");
    assert_eq!(after_snapshot.live_status, persisted.live_status);

    jobs.apply_print_report(ApplyPrintReport {
        tenant_id: tenant.id,
        agent_id: agent.id,
        serial: serial.clone(),
        task_id: None,
        job_id: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: None,
        percent: None,
        remaining_time_minutes: None,
        current_layer: None,
        total_layers: None,
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-07-09T10:01:00Z".to_string(),
    })
    .await
    .unwrap();

    let preserved = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(preserved.printer.status, "unknown");
    assert_eq!(preserved.printer.last_seen_at, "2026-07-09T10:01:00Z");
    assert_eq!(preserved.live_status, persisted.live_status);

    jobs.apply_print_report(ApplyPrintReport {
        hms: Some(Vec::new()),
        observed_at: "2026-07-09T10:02:00Z".to_string(),
        ..ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial,
            task_id: None,
            job_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: None,
            subtask_name: None,
            gcode_state: None,
            percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            hms: None,
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: String::new(),
        }
    })
    .await
    .unwrap();

    let cleared = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(cleared.live_status.progress_percent, Some(42));
    assert!(cleared.live_status.hms.is_empty());
}

#[tokio::test]
async fn sqlite_print_reports_merge_printer_live_status_without_a_job() {
    exercise_printer_live_status(sqlite_database().await).await;
}

#[tokio::test]
async fn invalid_persisted_printer_hms_is_reported_with_context() {
    let (database, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("invalid-hms", "Invalid HMS").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database")
    };
    sqlx::query("UPDATE printers SET hms_json = '{' WHERE id = ?1")
        .bind(printer_id)
        .execute(pool)
        .await
        .unwrap();

    let err = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("failed to read printer HMS"));
}
