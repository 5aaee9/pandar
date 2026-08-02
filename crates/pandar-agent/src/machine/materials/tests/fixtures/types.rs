use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TestMaterialPatch {
    #[serde(rename = "type")]
    pub(crate) document_type: String,
    pub(crate) cfg: Option<String>,
    pub(crate) aux: Option<String>,
    pub(crate) stat: Option<String>,
    pub(crate) filament_switch_installed: Option<bool>,
    #[serde(default)]
    pub(crate) ams_units: Vec<TestAmsUnit>,
    pub(crate) external_spools: Option<Vec<TestExternalSpool>>,
    pub(crate) replace_external_spools: Option<bool>,
    pub(crate) active_tray: Option<TestActiveTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TestAmsUnit {
    pub(crate) unit_id: String,
    pub(crate) unit_kind: Option<String>,
    pub(crate) info: Option<String>,
    #[serde(default)]
    pub(crate) trays: Vec<TestAmsTray>,
    pub(crate) replace_trays: Option<bool>,
    pub(crate) humidity: Option<f64>,
    pub(crate) humidity_level: Option<f64>,
    pub(crate) temperature_celsius: Option<f64>,
    pub(crate) toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TestAmsTray {
    pub(crate) tray_id: String,
    pub(crate) exists: Option<bool>,
    pub(crate) global_tray_id: Option<u64>,
    pub(crate) filament_id: Option<String>,
    pub(crate) setting_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) material_type: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) multi_color: Option<Vec<String>>,
    pub(crate) remaining_estimate: Option<String>,
    pub(crate) k_value: Option<String>,
    pub(crate) state: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TestExternalSpool {
    pub(crate) external_id: String,
    pub(crate) exists: Option<bool>,
    pub(crate) tray_id: String,
    pub(crate) setting_id: Option<String>,
    pub(crate) filament_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) material_type: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) remaining_estimate: Option<String>,
    pub(crate) toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum TestActiveTray {
    Ams {
        global_tray_id: i64,
        ams_id: Option<String>,
        tray_id: Option<String>,
    },
    AmsHt {
        global_tray_id: Option<u64>,
        ams_id: String,
        tray_id: String,
    },
    External {
        external_id: String,
        tray_id: String,
        global_tray_id: Option<u64>,
    },
}
