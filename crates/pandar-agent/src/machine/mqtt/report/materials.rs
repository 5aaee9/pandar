use std::collections::BTreeMap;

use serde::{Deserialize, de::IgnoredAny};
use serde_json::Number;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MaterialsReport {
    #[serde(default)]
    pub(in crate::machine) print: Option<PrintMaterialsReport>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine) struct PrintMaterialsReport {
    #[serde(default)]
    pub(in crate::machine) cfg: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) aux: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) stat: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) ams: Option<AmsReport>,
    #[serde(default)]
    pub(in crate::machine) vt_tray: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(in crate::machine) vir_slot: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(in crate::machine) nozzle_temper2: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) right_nozzle_temper: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) nozzles: Option<Vec<NozzleReport>>,
}

#[derive(Debug, Deserialize)]
pub(in crate::machine) struct NozzleReport {
    #[serde(flatten)]
    _fields: BTreeMap<String, IgnoredAny>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine) struct AmsReport {
    #[serde(default)]
    pub(in crate::machine) ams: Vec<AmsUnitReport>,
    #[serde(default)]
    pub(in crate::machine) tray_now: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) power_on_flag: Option<bool>,
    #[serde(default)]
    pub(in crate::machine) tray_exist_bits: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) vt_tray: Option<ExternalMaterialSource>,
    #[serde(default)]
    pub(in crate::machine) vir_slot: Option<ExternalMaterialSource>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine) struct AmsUnitReport {
    #[serde(default)]
    pub(in crate::machine) id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) ams_id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) info: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) humidity: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) humidity_raw: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) temperature_celsius: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) temp: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) dry_time: Option<ScalarValue>,
    #[serde(default, rename = "tray")]
    pub(in crate::machine) trays: Vec<MaterialSlotReport>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine) struct MaterialSlotReport {
    #[serde(default)]
    pub(in crate::machine) id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) external_id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) state: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_info_idx: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) setting_id: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_type: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tag_uid: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_uuid: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_sub_brands: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) remain: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) k: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) k_value: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) tray_color: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) cols: Option<ColorSource>,
    #[serde(default)]
    pub(in crate::machine) toolhead: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine) extruder_id: Option<ScalarValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine) enum ExternalMaterialSource {
    Array(Vec<MaterialSlotReport>),
    Object(Box<MaterialSlotReport>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(in crate::machine) enum ScalarValue {
    String(String),
    Number(Number),
    Bool(bool),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(in crate::machine) enum ColorSource {
    Single(ScalarValue),
    List(Vec<ScalarValue>),
}

impl ScalarValue {
    pub(in crate::machine) fn string(&self) -> Option<String> {
        match self {
            Self::String(raw) => {
                let trimmed = raw.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_owned())
            }
            Self::Number(value) => Some(value.to_string()),
            Self::Bool(value) => Some(value.to_string()),
        }
    }

    pub(in crate::machine) fn number(&self) -> Option<Number> {
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

    pub(in crate::machine) fn parse_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.as_i64().or_else(|| number.as_u64()?.try_into().ok()),
            Self::String(raw) => raw.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }

    pub(in crate::machine) fn parse_u64_or_hex(&self) -> Option<u64> {
        match self {
            Self::Number(number) => number.as_u64(),
            Self::String(raw) => {
                let trimmed = raw.trim();
                let hex = trimmed
                    .strip_prefix("0x")
                    .or_else(|| trimmed.strip_prefix("0X"))
                    .unwrap_or(trimmed);
                u64::from_str_radix(hex, 16).ok()
            }
            Self::Bool(_) => None,
        }
    }
}
