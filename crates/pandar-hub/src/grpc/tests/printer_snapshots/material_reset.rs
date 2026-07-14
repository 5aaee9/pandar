use serde::{Deserialize, Serialize};

use super::*;

#[tokio::test]
async fn authoritative_connection_snapshot_discards_materials_from_previous_agent() {
    let state = fixture_state().await;
    let (tenant_id, previous_agent_id) = tenant_agent(&state).await;
    let current_agent = state
        .agents()
        .create(tenant_id, "current-agent")
        .await
        .unwrap();
    let token = register_test_session(&state, tenant_id, current_agent.id).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, previous_agent_id)
        .await
        .unwrap();
    let serial = format!("serial-{printer_id}");
    state
        .materials()
        .upsert_from_patch(MaterialPatchInput {
            tenant_id,
            agent_id: previous_agent_id,
            printer_id: printer_id.clone(),
            serial_number: serial.clone(),
            printer_materials_json: material_patch("2026-07-15T00:00:00Z", &["0", "1", "128"]),
        })
        .await
        .unwrap();

    let mut connection = snapshot(&serial, "Printer", "X2D", "IDLE");
    connection.connection_authoritative = true;
    handle_snapshot(&state, tenant_id, current_agent.id, token, connection)
        .await
        .unwrap();
    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );

    state
        .materials()
        .upsert_from_patch(MaterialPatchInput {
            tenant_id,
            agent_id: current_agent.id,
            printer_id: printer_id.clone(),
            serial_number: serial,
            printer_materials_json: material_patch("2026-07-15T00:01:00Z", &["0"]),
        })
        .await
        .unwrap();
    let current = state
        .materials()
        .latest_for_printer(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let units: Vec<PersistedUnit> =
        serde_json::from_value(serde_json::to_value(current.ams_units).unwrap()).unwrap();

    assert_eq!(
        units,
        vec![PersistedUnit {
            unit_id: "0".into()
        }]
    );
}

fn material_patch(observed_at: &str, unit_ids: &[&str]) -> String {
    serde_json::to_string(&MaterialPatch {
        kind: "printer_material_patch",
        observed_at,
        ams_units: unit_ids
            .iter()
            .map(|unit_id| MaterialUnit {
                unit_id,
                trays: Vec::new(),
            })
            .collect(),
    })
    .unwrap()
}

#[derive(Serialize)]
struct MaterialPatch<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    ams_units: Vec<MaterialUnit<'a>>,
}

#[derive(Serialize)]
struct MaterialUnit<'a> {
    unit_id: &'a str,
    trays: Vec<MaterialTray>,
}

#[derive(Serialize)]
struct MaterialTray;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PersistedUnit {
    unit_id: String,
}
