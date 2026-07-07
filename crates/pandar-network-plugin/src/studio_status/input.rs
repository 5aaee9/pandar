use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(super) struct PrinterStatus {
    #[serde(default)]
    pub(super) dev_model_name: Option<Scalar>,
    #[serde(default)]
    pub(super) nozzle_temperatures: Vec<NozzleTemperature>,
    #[serde(default)]
    pub(super) active_nozzle: Option<Scalar>,
    #[serde(default)]
    pub(super) bed_temperature_celsius: Option<Scalar>,
    #[serde(default)]
    pub(super) bed_target_temperature_celsius: Option<Scalar>,
    #[serde(default)]
    pub(super) chamber_temperature_celsius: Option<Scalar>,
    #[serde(default)]
    pub(super) chamber_light_on: bool,
    #[serde(default)]
    pub(super) materials: Option<Materials>,
}

#[derive(Default, Deserialize)]
pub(super) struct NozzleTemperature {
    #[serde(default)]
    pub(super) label: Option<Scalar>,
    #[serde(default)]
    pub(super) current_celsius: Option<Scalar>,
    #[serde(default)]
    pub(super) target_celsius: Option<Scalar>,
}

#[derive(Default, Deserialize)]
pub(super) struct Materials {
    #[serde(default)]
    pub(super) ams_units: Vec<AmsUnit>,
    #[serde(default)]
    pub(super) external_spools: Vec<MaterialTray>,
    #[serde(default)]
    pub(super) active_tray: Option<ActiveTray>,
}

#[derive(Default, Deserialize)]
pub(super) struct AmsUnit {
    #[serde(default)]
    pub(super) unit_id: Option<Scalar>,
    #[serde(default)]
    pub(super) humidity: Option<Scalar>,
    #[serde(default)]
    pub(super) humidity_level: Option<Scalar>,
    #[serde(default)]
    pub(super) temperature_celsius: Option<Scalar>,
    #[serde(default)]
    pub(super) toolhead: Option<Scalar>,
    #[serde(default)]
    pub(super) trays: Vec<MaterialTray>,
}

#[derive(Default, Deserialize)]
pub(super) struct MaterialTray {
    #[serde(default)]
    pub(super) tray_id: Option<Scalar>,
    #[serde(default)]
    pub(super) global_tray_id: Option<Scalar>,
    #[serde(default)]
    pub(super) external_id: Option<Scalar>,
    #[serde(default)]
    pub(super) filament_id: Option<Scalar>,
    #[serde(default, rename = "type")]
    pub(super) filament_type: Option<Scalar>,
    #[serde(default)]
    pub(super) color: Option<Scalar>,
    #[serde(default)]
    pub(super) k_value: Option<Scalar>,
    #[serde(default)]
    pub(super) remaining_estimate: Option<Scalar>,
    #[serde(default)]
    pub(super) toolhead: Option<Scalar>,
}

#[derive(Default, Deserialize)]
pub(super) struct ActiveTray {
    #[serde(default)]
    pub(super) kind: Option<Scalar>,
    #[serde(default)]
    pub(super) ams_id: Option<Scalar>,
    #[serde(default)]
    pub(super) tray_id: Option<Scalar>,
    #[serde(default)]
    pub(super) global_tray_id: Option<Scalar>,
    #[serde(default)]
    pub(super) external_id: Option<Scalar>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum Scalar {
    String(String),
    Number(serde_json::Number),
    Bool(bool),
}

impl Scalar {
    pub(super) fn text(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
        }
    }
}
