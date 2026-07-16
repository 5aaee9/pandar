use serde::Serialize;

use super::*;
use crate::repositories::{
    MaterialPatchInput, MaterialPatchOutcome, test_helpers::insert_printer_fixture,
};

mod fixtures;
mod log_capture;
mod merge;

use fixtures::*;

#[tokio::test]
async fn material_repository_reports_changed_unchanged_empty_invalid_and_older_outcomes() {
    let (materials, tenant, agent, printer_id) = fixture().await;

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
            .upsert_from_patch_outcome(material_outcome_input(
                tenant.id,
                agent.id,
                &printer_id,
                WrongMaterialPatchFixture { kind: "wrong" },
            ))
            .await
            .unwrap(),
        MaterialPatchOutcome::Invalid { .. }
    ));

    let changed = materials
        .upsert_from_patch_outcome(material_outcome_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-07-02T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    assert!(matches!(changed, MaterialPatchOutcome::Changed(_)));
    let unchanged = materials
        .upsert_from_patch_outcome(material_outcome_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-07-02T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    assert!(matches!(unchanged, MaterialPatchOutcome::Unchanged(_)));
    let older = materials
        .upsert_from_patch_outcome(material_outcome_input(
            tenant.id,
            agent.id,
            &printer_id,
            patch("2026-07-01T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")]),
        ))
        .await
        .unwrap();
    assert!(matches!(older, MaterialPatchOutcome::Older));
}

#[derive(Debug, Serialize)]
struct WrongMaterialPatchFixture {
    #[serde(rename = "type")]
    kind: &'static str,
}

fn material_outcome_input(
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: &str,
    patch: impl Serialize,
) -> MaterialPatchInput {
    MaterialPatchInput {
        tenant_id,
        agent_id,
        printer_id: printer_id.to_owned(),
        serial_number: "serial".to_owned(),
        printer_materials_json: serde_json::to_string(&patch).unwrap(),
    }
}

#[tokio::test]
async fn filament_switch_state_persists_across_partial_patches_and_explicit_false() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                filament_switch_installed: Some(true),
                ..material_patch("2026-07-16T00:00:00Z")
            },
        ))
        .await
        .unwrap();
    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            material_patch("2026-07-16T00:01:00Z"),
        ))
        .await
        .unwrap();
    assert_eq!(
        materials
            .latest_for_printer(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap()
            .filament_switch_installed,
        Some(true)
    );

    let snapshot = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                filament_switch_installed: Some(false),
                ..material_patch("2026-07-16T00:02:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.filament_switch_installed, Some(false));
}

#[tokio::test]
async fn studio_status_flags_persist_raw_values_across_partial_patches() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                cfg: Some("8000000000000001"),
                aux: Some("A4003001"),
                stat: Some("1000000001"),
                ..material_patch("2026-07-16T01:00:00Z")
            },
        ))
        .await
        .unwrap();
    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            material_patch("2026-07-16T01:01:00Z"),
        ))
        .await
        .unwrap();
    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                cfg: Some(""),
                aux: Some(""),
                stat: Some(""),
                ..material_patch("2026-07-16T01:02:00Z")
            },
        ))
        .await
        .unwrap();

    let snapshot = materials
        .latest_for_printer(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.cfg.as_deref(), Some(""));
    assert_eq!(snapshot.aux.as_deref(), Some(""));
    assert_eq!(snapshot.stat.as_deref(), Some(""));
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
                ..patch_input(
                    tenant.id,
                    agent.id,
                    &printer_id,
                    material_patch("2026-06-23T00:00:00Z")
                )
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
            ..patch_input(
                tenant.id,
                agent.id,
                &printer_id,
                material_patch("2026-06-23T00:00:00Z"),
            )
        })
        .await
        .unwrap();
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored material patch"));
    assert!(!captured.contains("password-secret"));
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
            MaterialPatchFixture {
                cfg: Some("8000000000000001"),
                aux: Some("A4003001"),
                stat: Some("1000000001"),
                ..patch("2026-06-23T00:00:00Z", &[tray("0", "0", "PLA", "FF0000")])
            },
        ))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.cfg.as_deref(), Some("8000000000000001"));
    assert_eq!(snapshot.aux.as_deref(), Some("A4003001"));
    assert_eq!(snapshot.stat.as_deref(), Some("1000000001"));
    assert_eq!(
        materials
            .latest_for_printer(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap(),
        snapshot
    );
}
