use serde::Serialize;

use super::super::{AgentRepository, MaterialRepository, TenantRepository, material_repositories};
use crate::repositories::{MaterialPatchInput, test_helpers::insert_printer_fixture};

pub(super) fn patch_input(
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: &str,
    patch: impl Serialize,
) -> MaterialPatchInput {
    MaterialPatchInput {
        tenant_id,
        agent_id,
        printer_id: printer_id.to_string(),
        serial_number: format!("serial-{printer_id}"),
        printer_materials_json: serde_json::to_string(&patch).unwrap(),
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

#[derive(Clone, Debug, Serialize)]
pub(super) struct PatchTray<'a> {
    unit_id: &'a str,
    tray_id: &'a str,
    #[serde(rename = "type")]
    filament_type: &'a str,
    color: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct PatchAmsUnit<'a> {
    unit_id: &'a str,
    trays: Vec<PatchTray<'a>>,
}

#[derive(Debug, Serialize)]
pub(super) struct MaterialPatchFixture<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    ams_units: Vec<PatchAmsUnit<'a>>,
    external_spools: Vec<PatchExternalSpool>,
}

#[derive(Debug, Serialize)]
pub(super) struct PatchExternalSpool;

pub(super) fn patch<'a>(observed_at: &'a str, trays: &[PatchTray<'a>]) -> MaterialPatchFixture<'a> {
    MaterialPatchFixture {
        kind: "printer_material_patch",
        observed_at,
        ams_units: vec![PatchAmsUnit {
            unit_id: "0",
            trays: trays.to_vec(),
        }],
        external_spools: Vec::new(),
    }
}

pub(super) fn tray<'a>(
    unit_id: &'a str,
    tray_id: &'a str,
    filament_type: &'a str,
    color: &'a str,
) -> PatchTray<'a> {
    PatchTray {
        unit_id,
        tray_id,
        filament_type,
        color,
    }
}
