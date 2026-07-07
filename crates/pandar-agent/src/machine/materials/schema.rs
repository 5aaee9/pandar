use serde::Deserialize;
use serde_json::{Number, Value};

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
    pub(super) nozzle_temper2: Option<ScalarValue>,
    #[serde(default)]
    pub(super) right_nozzle_temper: Option<ScalarValue>,
    #[serde(default)]
    pub(super) nozzles: Option<Vec<Value>>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AmsReport {
    #[serde(default)]
    pub(super) ams: Vec<AmsUnitReport>,
    #[serde(default)]
    pub(super) tray_now: Option<ScalarValue>,
    #[serde(default)]
    pub(super) power_on_flag: Option<bool>,
    #[serde(default)]
    pub(super) tray_exist_bits: Option<ScalarValue>,
    #[serde(default)]
    pub(super) vt_tray: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(super) vir_slot: Option<ExternalMaterialSource>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AmsUnitReport {
    #[serde(default)]
    pub(super) id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) ams_id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) info: Option<ScalarValue>,
    #[serde(default)]
    pub(super) humidity: Option<ScalarValue>,
    #[serde(default)]
    pub(super) humidity_raw: Option<ScalarValue>,
    #[serde(default)]
    pub(super) temperature_celsius: Option<ScalarValue>,
    #[serde(default)]
    pub(super) temp: Option<ScalarValue>,
    #[serde(default, rename = "tray")]
    pub(super) trays: Vec<MaterialSlotReport>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct MaterialSlotReport {
    #[serde(default)]
    pub(super) id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) external_id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) state: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_info_idx: Option<ScalarValue>,
    #[serde(default)]
    pub(super) setting_id: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_type: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tag_uid: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_uuid: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_sub_brands: Option<ScalarValue>,
    #[serde(default)]
    pub(super) remain: Option<ScalarValue>,
    #[serde(default)]
    pub(super) k: Option<ScalarValue>,
    #[serde(default)]
    pub(super) k_value: Option<ScalarValue>,
    #[serde(default)]
    pub(super) tray_color: Option<ScalarValue>,
    #[serde(default)]
    pub(super) cols: Option<ColorSource>,
    #[serde(default)]
    pub(super) toolhead: Option<ScalarValue>,
    #[serde(default)]
    pub(super) extruder_id: Option<ScalarValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ExternalMaterialSource {
    Array(Vec<MaterialSlotReport>),
    Object(Box<MaterialSlotReport>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum ScalarValue {
    String(String),
    Number(Number),
    Bool(bool),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum ColorSource {
    Single(ScalarValue),
    List(Vec<ScalarValue>),
}

impl ScalarValue {
    pub(super) fn string(&self) -> Option<String> {
        match self {
            Self::String(raw) => {
                let trimmed = raw.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_owned())
            }
            Self::Number(_) | Self::Bool(_) => Some(self.to_json_value().to_string()),
        }
    }

    pub(super) fn number(&self) -> Option<Number> {
        match self {
            Self::Number(number) => Some(number.clone()),
            Self::String(raw) => {
                let raw = raw.trim();
                raw.parse::<i64>()
                    .ok()
                    .map(Number::from)
                    .or_else(|| raw.parse::<f64>().ok().and_then(Number::from_f64))
            }
            Self::Bool(_) => None,
        }
    }

    pub(super) fn parse_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.as_i64().or_else(|| number.as_u64()?.try_into().ok()),
            Self::String(raw) => raw.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }

    pub(super) fn parse_u64_or_hex(&self) -> Option<u64> {
        match self {
            Self::Number(number) => number.as_u64(),
            Self::String(raw) => {
                let trimmed = raw.trim();
                let hex = trimmed
                    .strip_prefix("0x")
                    .or_else(|| trimmed.strip_prefix("0X"));
                match hex {
                    Some(hex) => u64::from_str_radix(hex, 16).ok(),
                    None => trimmed.parse::<u64>().ok(),
                }
            }
            Self::Bool(_) => None,
        }
    }

    fn to_json_value(&self) -> Value {
        match self {
            Self::String(value) => Value::String(value.clone()),
            Self::Number(value) => Value::Number(value.clone()),
            Self::Bool(value) => Value::Bool(*value),
        }
    }
}
