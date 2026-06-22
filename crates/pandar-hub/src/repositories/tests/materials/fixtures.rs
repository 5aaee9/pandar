use serde_json::{Value, json};

use super::super::{AgentRepository, MaterialRepository, TenantRepository, material_repositories};
use crate::repositories::{MaterialPatchInput, test_helpers::insert_printer_fixture};

pub(super) fn patch_input(
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: &str,
    patch: Value,
) -> MaterialPatchInput {
    MaterialPatchInput {
        tenant_id,
        agent_id,
        printer_id: printer_id.to_string(),
        serial_number: format!("serial-{printer_id}"),
        printer_materials_json: patch.to_string(),
    }
}

pub(super) async fn fixture() -> (
    MaterialRepository,
    pandar_core::Tenant,
    pandar_core::Agent,
    String,
) {
    let (database, tenants, agents, _, materials): (
        _,
        TenantRepository,
        AgentRepository,
        _,
        MaterialRepository,
    ) = material_repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    (materials, tenant, agent, printer_id)
}

pub(super) fn patch(observed_at: &str, trays: &[Value]) -> Value {
    json!({
        "type": "printer_material_patch",
        "observed_at": observed_at,
        "ams_units": [{"unit_id": "0", "trays": trays}],
        "external_spools": []
    })
}

pub(super) fn tray(unit_id: &str, tray_id: &str, filament_type: &str, color: &str) -> Value {
    json!({
        "unit_id": unit_id,
        "tray_id": tray_id,
        "type": filament_type,
        "color": color
    })
}
