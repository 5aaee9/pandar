use serde::{Deserialize, Serialize};

use super::super::{AgentRepository, MaterialRepository, TenantRepository, material_repositories};
use crate::repositories::{
    MaterialPatchInput, MaterialSnapshot, test_helpers::insert_printer_fixture,
};

pub(super) fn ams_units(snapshot: &MaterialSnapshot) -> Vec<TestMaterialUnit> {
    serde_json::from_value(serde_json::to_value(&snapshot.ams_units).unwrap()).unwrap()
}

pub(super) fn external_spools(snapshot: &MaterialSnapshot) -> Vec<TestExternalSpool> {
    serde_json::from_value(serde_json::to_value(&snapshot.external_spools).unwrap()).unwrap()
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestMaterialUnit {
    pub(super) unit_id: String,
    pub(super) humidity: Option<f64>,
    #[serde(default)]
    pub(super) trays: Vec<TestMaterialTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestMaterialTray {
    pub(super) tray_id: String,
    #[serde(rename = "type")]
    pub(super) material_type: Option<String>,
    pub(super) color: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestExternalSpool {
    pub(super) external_id: String,
    pub(super) tray_id: String,
    #[serde(rename = "type")]
    pub(super) material_type: Option<String>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unit_id: Option<&'a str>,
    pub(super) tray_id: &'a str,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) filament_type: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) color: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub(super) struct PatchAmsUnit<'a> {
    pub(super) unit_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) humidity: Option<Option<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) replace_trays: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) access_code: Option<&'a str>,
    pub(super) trays: Vec<PatchTray<'a>>,
}

#[derive(Debug, Serialize)]
pub(super) struct MaterialPatchFixture<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) observed_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ams_units: Option<Vec<PatchAmsUnit<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) external_spools: Option<Vec<PatchExternalSpool<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) replace_external_spools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) active_tray: Option<Option<PatchActiveTray<'a>>>,
}

#[derive(Debug, Serialize)]
pub(super) struct PatchExternalSpool<'a> {
    pub(super) external_id: &'a str,
    pub(super) tray_id: &'a str,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) filament_type: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub(super) struct PatchActiveTray<'a> {
    pub(super) kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ams_id: Option<&'a str>,
    pub(super) tray_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) token: Option<&'a str>,
}

pub(super) fn material_patch(observed_at: &str) -> MaterialPatchFixture<'_> {
    MaterialPatchFixture {
        kind: "printer_material_patch",
        observed_at,
        ams_units: None,
        external_spools: None,
        replace_external_spools: None,
        active_tray: None,
    }
}

pub(super) fn patch<'a>(observed_at: &'a str, trays: &[PatchTray<'a>]) -> MaterialPatchFixture<'a> {
    MaterialPatchFixture {
        ams_units: Some(vec![PatchAmsUnit {
            unit_id: "0",
            humidity: None,
            replace_trays: None,
            access_code: None,
            trays: trays.to_vec(),
        }]),
        external_spools: Some(Vec::new()),
        ..material_patch(observed_at)
    }
}

pub(super) fn tray<'a>(
    unit_id: &'a str,
    tray_id: &'a str,
    filament_type: &'a str,
    color: &'a str,
) -> PatchTray<'a> {
    PatchTray {
        unit_id: Some(unit_id),
        tray_id,
        filament_type: Some(Some(filament_type)),
        color: Some(Some(color)),
        password: None,
        name: None,
    }
}

pub(super) fn tray_without_unit<'a>(
    tray_id: &'a str,
    filament_type: Option<Option<&'a str>>,
    color: Option<Option<&'a str>>,
) -> PatchTray<'a> {
    PatchTray {
        unit_id: None,
        tray_id,
        filament_type,
        color,
        password: None,
        name: None,
    }
}

pub(super) fn ams_unit<'a>(unit_id: &'a str, trays: Vec<PatchTray<'a>>) -> PatchAmsUnit<'a> {
    PatchAmsUnit {
        unit_id,
        humidity: None,
        replace_trays: None,
        access_code: None,
        trays,
    }
}

pub(super) fn external_spool<'a>(
    external_id: &'a str,
    tray_id: &'a str,
    filament_type: Option<Option<&'a str>>,
) -> PatchExternalSpool<'a> {
    PatchExternalSpool {
        external_id,
        tray_id,
        filament_type,
        auth: None,
    }
}

pub(super) fn active_ams_tray<'a>(ams_id: &'a str, tray_id: &'a str) -> PatchActiveTray<'a> {
    PatchActiveTray {
        kind: "ams",
        ams_id: Some(ams_id),
        tray_id,
        token: None,
    }
}
