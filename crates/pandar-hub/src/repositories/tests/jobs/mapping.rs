use super::*;

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
    assert_eq!(
        payload["artifact_download_path"],
        format!(
            "/api/v1/agents/{}/artifacts/{}",
            agent.id, created.artifact.id
        )
    );
    assert_eq!(payload["storage_path"], created.artifact.storage_path);
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
async fn job_repository_persists_mapping_json_null_and_empty_distinctly() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let neither = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let mut empty = create_input(tenant.id, agent.id, &printer_id, "artifact-2");
    empty.ams_mapping_json = Some("[]".to_string());
    empty.ams_mapping2_json = Some("[]".to_string());
    let empty = jobs.create_print_job(empty).await.unwrap();

    let neither = jobs
        .get_for_tenant(tenant.id, neither.job.id)
        .await
        .unwrap()
        .unwrap();
    let empty = jobs
        .get_for_tenant(tenant.id, empty.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(neither.job.ams_mapping_json, None);
    assert_eq!(neither.job.ams_mapping2_json, None);
    assert_eq!(empty.job.ams_mapping_json.as_deref(), Some("[]"));
    assert_eq!(empty.job.ams_mapping2_json.as_deref(), Some("[]"));

    let payloads = queued_payloads(&commands, tenant.id, agent.id).await;
    assert_eq!(payloads[0]["ams_mapping_json"], Value::Null);
    assert_eq!(payloads[1]["ams_mapping_json"], "[]");
}

#[tokio::test]
async fn job_repository_rejects_mapping_json_over_32_entries() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.ams_mapping_json = Some(format!(
        "[{}]",
        std::iter::repeat_n("0", 33).collect::<Vec<_>>().join(",")
    ));
    let err = jobs.create_print_job(input).await.unwrap_err();

    assert!(matches!(err, RepositoryError::Database(_)));
    assert!(format!("{err:#}").contains("ams_mapping_json must not contain more than 32 entries"));

    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-2");
    input.ams_mapping2_json = Some(format!(
        "[{}]",
        std::iter::repeat_n(r#"{"ams_id":0,"slot_id":0}"#, 33)
            .collect::<Vec<_>>()
            .join(",")
    ));
    let err = jobs.create_print_job(input).await.unwrap_err();

    assert!(matches!(err, RepositoryError::Database(_)));
    assert!(format!("{err:#}").contains("ams_mapping2_json must not contain more than 32 entries"));
}

#[tokio::test]
async fn job_repository_accepts_mapping2_external_slot_outside_ams_tray_range() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.ams_mapping2_json = Some(r#"[{"ams_id":254,"slot_id":8}]"#.to_string());
    let created = jobs.create_print_job(input).await.unwrap();
    let payloads = queued_payloads(&commands, tenant.id, agent.id).await;

    assert_eq!(
        created.job.ams_mapping2_json.as_deref(),
        Some(r#"[{"ams_id":254,"slot_id":8}]"#)
    );
    assert_eq!(
        payloads[0]["ams_mapping2_json"],
        r#"[{"ams_id":254,"slot_id":8}]"#
    );
}

#[tokio::test]
async fn terminal_usage_ignores_normal_ams_mapping2_slot_outside_tray_range() {
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
    sqlx::query("UPDATE jobs SET ams_mapping2_json = ?2 WHERE id = ?1")
        .bind(created.job.id.to_string())
        .bind(r#"[{"ams_id":0,"slot_id":8}]"#)
        .execute(pool)
        .await
        .unwrap();

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            printer_materials_json: material_patch_json("2026-06-22T00:00:00Z"),
            ..report_input(
                tenant.id,
                agent.id,
                &printer_id,
                Some(created.job.id),
                None,
                "FINISH",
            )
        })
        .await
        .unwrap();

    assert_eq!(applied.job.unwrap().job.filament_usage, Vec::new());
}

#[tokio::test]
async fn terminal_report_derives_usage_from_mapping_and_material_snapshot_idempotently() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.ams_mapping_json = Some("[0,254,255,128,0]".to_string());
    let created = jobs.create_print_job(input).await.unwrap();

    let terminal = ApplyPrintReport {
        printer_materials_json: material_patch_json("2026-06-22T00:00:00Z"),
        ..report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(created.job.id),
            None,
            "FINISH",
        )
    };
    let applied = jobs.apply_print_report(terminal.clone()).await.unwrap();
    let replay = jobs.apply_print_report(terminal).await.unwrap();

    let usage = applied.job.unwrap().job.filament_usage;
    assert_eq!(usage.len(), 4);
    assert_eq!(usage[0].slot_index, 0);
    assert_eq!(usage[0].source, "ams_mapping");
    assert_eq!(usage[0].ams_id.as_deref(), Some("0"));
    assert_eq!(usage[0].tray_id.as_deref(), Some("0"));
    assert_eq!(usage[0].global_tray_id, Some(0));
    assert_eq!(usage[0].filament_type.as_deref(), Some("PLA"));
    assert_eq!(usage[1].external_id.as_deref(), Some("254"));
    assert_eq!(usage[1].tray_id.as_deref(), Some("0"));
    assert_eq!(usage[1].filament_type.as_deref(), Some("PETG"));
    assert_eq!(usage[2].ams_id.as_deref(), Some("128"));
    assert_eq!(usage[2].global_tray_id, None);
    assert_eq!(usage[3].slot_index, 4);
    assert_eq!(usage[3].filament_type.as_deref(), Some("PLA"));
    assert_eq!(replay.job.unwrap().job.filament_usage.len(), 4);
    assert_eq!(
        jobs.filament_usage_count(tenant.id, created.job.id)
            .await
            .unwrap(),
        4
    );
}

#[tokio::test]
async fn mapping2_takes_precedence_and_external_slots_match_materials() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.ams_mapping_json = Some("[0,0,0]".to_string());
    input.ams_mapping2_json = Some(
        r#"[{"ams_id":254,"slot_id":8},{"ams_id":255,"slot_id":255},{"ams_id":2,"slot_id":3}]"#
            .to_string(),
    );
    let created = jobs.create_print_job(input).await.unwrap();

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            printer_materials_json: material_patch_json("2026-06-22T00:00:00Z"),
            ..report_input(
                tenant.id,
                agent.id,
                &printer_id,
                Some(created.job.id),
                None,
                "FAILED",
            )
        })
        .await
        .unwrap();

    let usage = applied.job.unwrap().job.filament_usage;
    assert_eq!(usage.len(), 2);
    assert_eq!(usage[0].slot_index, 0);
    assert_eq!(usage[0].source, "ams_mapping2");
    assert_eq!(usage[0].external_id.as_deref(), Some("254"));
    assert_eq!(usage[0].tray_id.as_deref(), Some("8"));
    assert_eq!(usage[0].filament_type.as_deref(), Some("TPU"));
    assert_eq!(usage[1].slot_index, 2);
    assert_eq!(usage[1].source, "ams_mapping2");
    assert_eq!(usage[1].global_tray_id, Some(11));
}

#[tokio::test]
async fn running_report_updates_material_snapshot_without_deriving_usage() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.ams_mapping_json = Some("[0]".to_string());
    let created = jobs.create_print_job(input).await.unwrap();

    let applied = jobs
        .apply_print_report(ApplyPrintReport {
            printer_materials_json: material_patch_json("2026-06-22T00:00:00Z"),
            ..report_input(
                tenant.id,
                agent.id,
                &printer_id,
                Some(created.job.id),
                None,
                "RUNNING",
            )
        })
        .await
        .unwrap();

    assert_eq!(applied.job.unwrap().job.filament_usage, Vec::new());
    assert_eq!(
        jobs.filament_usage_count(tenant.id, created.job.id)
            .await
            .unwrap(),
        0
    );
    assert_eq!(material_snapshot_count(&database).await, 1);
}
