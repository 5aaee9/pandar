use serde::Deserialize;
use serde_json::json;

use super::*;
use crate::repositories::{
    MaterialPatchInput, MaterialPatchOutcome, MaterialSnapshot,
    test_helpers::insert_printer_fixture,
};

mod fixtures;
mod log_capture;

use fixtures::*;

fn ams_units(snapshot: &MaterialSnapshot) -> Vec<TestMaterialUnit> {
    serde_json::from_value(snapshot.ams_units.clone()).unwrap()
}

fn external_spools(snapshot: &MaterialSnapshot) -> Vec<TestExternalSpool> {
    serde_json::from_value(snapshot.external_spools.clone()).unwrap()
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialUnit {
    unit_id: String,
    humidity: Option<f64>,
    #[serde(default)]
    trays: Vec<TestMaterialTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialTray {
    tray_id: String,
    #[serde(rename = "type")]
    material_type: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestExternalSpool {
    external_id: String,
    tray_id: String,
    #[serde(rename = "type")]
    material_type: Option<String>,
}

#[tokio::test]
async fn material_repository_reports_changed_unchanged_empty_invalid_and_older_outcomes() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let input = |body: serde_json::Value| MaterialPatchInput {
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id: printer_id.clone(),
        serial_number: "serial".to_owned(),
        printer_materials_json: body.to_string(),
    };

    assert!(matches!(
        materials
            .upsert_from_patch_outcome(MaterialPatchInput {
                tenant_id: tenant.id,
                agent_id: agent.id,
                printer_id: printer_id.clone(),
                serial_number: "serial".to_owned(),
                printer_materials_json: String::new(),
            })
            .await
            .unwrap(),
        MaterialPatchOutcome::Empty
    ));
    assert!(matches!(
        materials
            .upsert_from_patch_outcome(input(json!({"type":"wrong"})))
            .await
            .unwrap(),
        MaterialPatchOutcome::Invalid { .. }
    ));

    let patch = |observed_at: &str| {
        json!({
            "type": "printer_material_patch",
            "observed_at": observed_at,
            "ams_units": [{"unit_id": "0", "trays": [{"tray_id": "0", "type": "PLA"}]}],
            "external_spools": []
        })
    };

    let changed = materials
        .upsert_from_patch_outcome(input(patch("2026-07-02T00:00:00Z")))
        .await
        .unwrap();
    assert!(matches!(changed, MaterialPatchOutcome::Changed(_)));
    let unchanged = materials
        .upsert_from_patch_outcome(input(patch("2026-07-02T00:00:00Z")))
        .await
        .unwrap();
    assert!(matches!(unchanged, MaterialPatchOutcome::Unchanged(_)));
    let older = materials
        .upsert_from_patch_outcome(input(patch("2026-07-01T00:00:00Z")))
        .await
        .unwrap();
    assert!(matches!(older, MaterialPatchOutcome::Older));
}

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
    assert_eq!(
        ams_units(&acme_snapshot)[0].trays[0]
            .material_type
            .as_deref(),
        Some("PLA")
    );
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

    let units = ams_units(&merged);
    let unit = &units[0];
    assert_eq!(unit.humidity, None);
    assert_eq!(unit.trays[0].material_type.as_deref(), Some("PLA"));
    assert_eq!(unit.trays[1].material_type.as_deref(), Some("ABS"));
    assert_eq!(unit.trays[1].color, None);
    assert_eq!(
        external_spools(&merged)[0].material_type.as_deref(),
        Some("PLA")
    );
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
    let created_units = ams_units(&created);
    let created_external_spools = external_spools(&created);
    assert_eq!(created_units[0].humidity, None);
    assert_eq!(created_units[0].trays[0].material_type, None);
    assert_eq!(created_units[0].trays[0].color.as_deref(), Some("FF0000"));
    assert_eq!(created_external_spools[0].material_type, None);

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
    let merged_units = ams_units(&merged);
    let merged_external_spools = external_spools(&merged);
    assert_eq!(merged_units[0].trays[1].material_type, None);
    assert_eq!(merged_units[0].trays[1].color.as_deref(), Some("00FF00"));
    assert_eq!(merged_external_spools[1].material_type, None);
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

    let replaced_units = ams_units(&replaced);
    let replaced_external_spools = external_spools(&replaced);
    assert_eq!(replaced_units[0].trays.len(), 1);
    assert_eq!(replaced_units[0].trays[0].tray_id, "1");
    assert_eq!(replaced_external_spools.len(), 1);
    assert_eq!(replaced_external_spools[0].tray_id, "1");
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

    assert_eq!(
        ams_units(&equal)[0].trays[0].material_type.as_deref(),
        Some("PETG")
    );
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
    assert_eq!(
        ams_units(&snapshot)[0].trays[0].material_type.as_deref(),
        Some("PLA")
    );
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
