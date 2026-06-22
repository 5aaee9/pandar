use serde_json::json;

use super::*;
use crate::repositories::{MaterialPatchInput, test_helpers::insert_printer_fixture};

mod fixtures;
mod log_capture;

use fixtures::*;

#[tokio::test]
async fn material_snapshots_are_scoped_to_tenant_and_printer() {
    let (database, tenants, agents, _, materials) = material_repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();
    let acme_printer = insert_printer_fixture(&database, acme.id, acme_agent.id)
        .await
        .unwrap();
    let beta_printer = insert_printer_fixture(&database, beta.id, beta_agent.id)
        .await
        .unwrap();

    materials
        .upsert_from_patch(patch_input(
            acme.id,
            acme_agent.id,
            &acme_printer,
            patch("2026-06-23T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    materials
        .upsert_from_patch(patch_input(
            beta.id,
            beta_agent.id,
            &beta_printer,
            patch("2026-06-23T00:01:00Z", &[tray("0", "0", "PETG", "00FF00")]),
        ))
        .await
        .unwrap();

    let acme_snapshot = materials
        .latest_for_printer(acme.id, &acme_printer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(acme_snapshot.ams_units[0]["trays"][0]["type"], "PLA");
    assert_eq!(materials.list_for_tenant(acme.id).await.unwrap().len(), 1);
    assert!(
        materials
            .latest_for_printer(acme.id, &beta_printer)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn invalid_material_json_is_ignored_without_changing_state() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-06-23T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    assert!(
        materials
            .upsert_from_patch(MaterialPatchInput {
                printer_materials_json:
                    r#"{"type":"printer_material_patch","observed_at":"bad","password":"secret"}"#
                        .to_string(),
                ..patch_input(tenant.id, agent.id, &printer_id, json!({}))
            })
            .await
            .unwrap()
            .is_none()
    );

    let snapshot = materials
        .latest_for_printer(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.observed_at, "2026-06-23T00:00:00Z");
    assert!(!snapshot.persisted_json().contains("secret"));
}

#[tokio::test]
async fn invalid_observed_at_does_not_log_credential_value() {
    let logs = log_capture::CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let (materials, tenant, agent, printer_id) = fixture().await;

    let _guard = tracing::subscriber::set_default(subscriber);
    materials
        .upsert_from_patch(MaterialPatchInput {
            printer_materials_json:
                r#"{"type":"printer_material_patch","observed_at":"password-secret"}"#.to_string(),
            ..patch_input(tenant.id, agent.id, &printer_id, json!({}))
        })
        .await
        .unwrap();
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored material patch"));
    assert!(!captured.contains("password-secret"));
}

#[tokio::test]
async fn partial_replay_merges_absent_null_and_concrete_fields() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{
                    "unit_id": "0",
                    "humidity": 30,
                    "trays": [tray("0", "0", "PLA", "FF0000"), tray("0", "1", "PETG", "00FF00")]
                }],
                "external_spools": [{"external_id": "254", "tray_id": "0", "type": "PLA"}],
                "active_tray": {"kind": "ams", "ams_id": "0", "tray_id": "0"}
            }),
        ))
        .await
        .unwrap();

    let merged = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{
                    "unit_id": "0",
                    "humidity": null,
                    "trays": [{"tray_id": "1", "type": "ABS", "color": null}]
                }],
                "active_tray": null
            }),
        ))
        .await
        .unwrap()
        .unwrap();

    let unit = &merged.ams_units[0];
    assert!(unit.get("humidity").is_none());
    assert_eq!(unit["trays"][0]["type"], "PLA");
    assert_eq!(unit["trays"][1]["type"], "ABS");
    assert!(unit["trays"][1].get("color").is_none());
    assert_eq!(merged.external_spools[0]["type"], "PLA");
    assert!(merged.active_tray.is_none());
}

#[tokio::test]
async fn first_snapshot_and_new_entries_drop_null_fields() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let created = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{
                    "unit_id": "0",
                    "humidity": null,
                    "trays": [{"tray_id": "0", "type": null, "color": "FF0000"}]
                }],
                "external_spools": [{"external_id": "254", "tray_id": "0", "type": null}]
            }),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(created.ams_units[0].get("humidity").is_none());
    assert!(created.ams_units[0]["trays"][0].get("type").is_none());
    assert_eq!(created.ams_units[0]["trays"][0]["color"], "FF0000");
    assert!(created.external_spools[0].get("type").is_none());

    let merged = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{
                    "unit_id": "0",
                    "trays": [{"tray_id": "1", "type": null, "color": "00FF00"}]
                }],
                "external_spools": [{"external_id": "254", "tray_id": "1", "type": null}]
            }),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(merged.ams_units[0]["trays"][1].get("type").is_none());
    assert_eq!(merged.ams_units[0]["trays"][1]["color"], "00FF00");
    assert!(merged.external_spools[1].get("type").is_none());
}

#[tokio::test]
async fn replacement_flags_remove_unmentioned_collections() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{"unit_id": "0", "trays": [tray("0", "0", "PLA", "FF0000"), tray("0", "1", "PETG", "00FF00")]}],
                "external_spools": [{"external_id": "254", "tray_id": "0"}, {"external_id": "254", "tray_id": "1"}]
            }),
        ))
        .await
        .unwrap();
    let replaced = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:01:00Z",
                "ams_units": [{"unit_id": "0", "replace_trays": true, "trays": [tray("0", "1", "ABS", "0000FF")]}],
                "replace_external_spools": true,
                "external_spools": [{"external_id": "254", "tray_id": "1"}]
            }),
        ))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(replaced.ams_units[0]["trays"].as_array().unwrap().len(), 1);
    assert_eq!(replaced.ams_units[0]["trays"][0]["tray_id"], "1");
    assert_eq!(replaced.external_spools.as_array().unwrap().len(), 1);
    assert_eq!(replaced.external_spools[0]["tray_id"], "1");
}

#[tokio::test]
async fn out_of_order_replay_is_ignored_but_equal_timestamp_is_accepted() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-06-23T00:02:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    assert!(
        materials
            .upsert_from_patch(patch_input(
                tenant.id,
                agent.id,
                &printer_id,
                patch("2026-06-23T00:01:00Z", &[tray("0", "0", "ABS", "0000FF")]),
            ))
            .await
            .unwrap()
            .is_none()
    );
    let equal = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-06-23T00:02:00Z", &[tray("0", "0", "PETG", "00FF00")]),
        ))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(equal.ams_units[0]["trays"][0]["type"], "PETG");
}

#[tokio::test]
async fn credential_shaped_keys_and_values_are_not_persisted() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let snapshot = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T00:00:00Z",
                "ams_units": [{
                    "unit_id": "0",
                    "access_code": "secret-code",
                    "trays": [{
                        "tray_id": "0",
                        "type": "PLA",
                        "password": "secret-password",
                        "name": "token-secret"
                    }]
                }],
                "external_spools": [{"external_id": "254", "tray_id": "0", "auth": "secret-auth"}],
                "active_tray": {"kind": "ams", "token": "secret-token", "tray_id": "0"}
            }),
        ))
        .await
        .unwrap()
        .unwrap();

    let persisted = snapshot.persisted_json();
    for needle in ["access_code", "password", "auth", "token", "secret"] {
        assert!(!persisted.contains(needle), "persisted sensitive {needle}");
    }
    assert_eq!(snapshot.ams_units[0]["trays"][0]["type"], "PLA");
}

#[tokio::test]
async fn postgres_material_repository_behavior_when_configured() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let materials = MaterialRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let snapshot = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-06-23T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        materials
            .latest_for_printer(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap(),
        snapshot
    );
}
