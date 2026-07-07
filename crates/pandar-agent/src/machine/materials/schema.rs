use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
pub(super) struct MaterialsReport {
    #[serde(default)]
    pub(super) print: Option<PrintMaterialsReport>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct PrintMaterialsReport {
    #[serde(default)]
    pub(super) ams: Option<AmsReport>,
    #[serde(default)]
    pub(super) vt_tray: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(super) vir_slot: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(super) nozzle_temper2: Option<Value>,
    #[serde(default)]
    pub(super) right_nozzle_temper: Option<Value>,
    #[serde(default)]
    pub(super) nozzles: Option<Vec<Value>>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AmsReport {
    #[serde(default)]
    pub(super) ams: Vec<AmsUnitReport>,
    #[serde(default)]
    pub(super) tray_now: Option<Value>,
    #[serde(default)]
    pub(super) power_on_flag: Option<bool>,
    #[serde(default)]
    pub(super) tray_exist_bits: Option<Value>,
    #[serde(default)]
    pub(super) vt_tray: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(super) vir_slot: Option<ExternalMaterialSource>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AmsUnitReport {
    #[serde(default)]
    pub(super) id: Option<Value>,
    #[serde(default)]
    pub(super) ams_id: Option<Value>,
    #[serde(default)]
    pub(super) info: Option<Value>,
    #[serde(default)]
    pub(super) humidity: Option<Value>,
    #[serde(default)]
    pub(super) humidity_raw: Option<Value>,
    #[serde(default)]
    pub(super) temperature_celsius: Option<Value>,
    #[serde(default)]
    pub(super) temp: Option<Value>,
    #[serde(default, rename = "tray")]
    pub(super) trays: Vec<MaterialSlotReport>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct MaterialSlotReport {
    #[serde(default)]
    pub(super) id: Option<Value>,
    #[serde(default)]
    pub(super) tray_id: Option<Value>,
    #[serde(default)]
    pub(super) external_id: Option<Value>,
    #[serde(default)]
    pub(super) state: Option<Value>,
    #[serde(default)]
    pub(super) tray_info_idx: Option<Value>,
    #[serde(default)]
    pub(super) setting_id: Option<Value>,
    #[serde(default)]
    pub(super) tray_type: Option<Value>,
    #[serde(default)]
    pub(super) tag_uid: Option<Value>,
    #[serde(default)]
    pub(super) tray_uuid: Option<Value>,
    #[serde(default)]
    pub(super) tray_sub_brands: Option<Value>,
    #[serde(default)]
    pub(super) remain: Option<Value>,
    #[serde(default)]
    pub(super) k: Option<Value>,
    #[serde(default)]
    pub(super) k_value: Option<Value>,
    #[serde(default)]
    pub(super) tray_color: Option<Value>,
    #[serde(default)]
    pub(super) cols: Option<Value>,
    #[serde(default)]
    pub(super) toolhead: Option<Value>,
    #[serde(default)]
    pub(super) extruder_id: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ExternalMaterialSource {
    Array(Vec<MaterialSlotReport>),
    Object(Box<MaterialSlotReport>),
}
