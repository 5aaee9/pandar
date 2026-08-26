use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::repositories::{MaterialJsonValue, MaterialSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventMaterials {
    pub ams_units: PrinterEventMaterialJson,
    pub external_spools: PrinterEventMaterialJson,
    pub active_tray: Option<PrinterEventMaterialJson>,
    pub filament_switch_installed: Option<bool>,
    pub cfg: Option<String>,
    pub aux: Option<String>,
    pub stat: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PrinterEventMaterialJson {
    Object(BTreeMap<String, PrinterEventMaterialJson>),
    Array(Vec<PrinterEventMaterialJson>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl From<MaterialSnapshot> for PrinterEventMaterials {
    fn from(snapshot: MaterialSnapshot) -> Self {
        Self {
            ams_units: PrinterEventMaterialJson::from(snapshot.ams_units).scrubbed(),
            external_spools: PrinterEventMaterialJson::from(snapshot.external_spools).scrubbed(),
            active_tray: snapshot.active_tray.map(scrub_material_json),
            filament_switch_installed: snapshot.filament_switch_installed,
            observed_at: snapshot.observed_at,
            cfg: snapshot.cfg,
            aux: snapshot.aux,
            stat: snapshot.stat,
        }
    }
}

fn scrub_material_json(value: MaterialJsonValue) -> PrinterEventMaterialJson {
    PrinterEventMaterialJson::from(value).scrubbed()
}

impl PrinterEventMaterialJson {
    fn scrubbed(self) -> Self {
        match self {
            Self::Array(values) => Self::Array(values.into_iter().map(Self::scrubbed).collect()),
            Self::Object(map) => Self::Object(
                map.into_iter()
                    .filter_map(|(key, value)| {
                        (!credential_key(&key)).then(|| (key, value.scrubbed()))
                    })
                    .collect(),
            ),
            value => value,
        }
    }
}

impl From<MaterialJsonValue> for PrinterEventMaterialJson {
    fn from(value: MaterialJsonValue) -> Self {
        match value {
            MaterialJsonValue::Object(object) => Self::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            MaterialJsonValue::Array(values) => {
                Self::Array(values.into_iter().map(Self::from).collect())
            }
            MaterialJsonValue::String(value) => Self::String(value),
            MaterialJsonValue::Number(value) => Self::Number(value),
            MaterialJsonValue::Bool(value) => Self::Bool(value),
            MaterialJsonValue::Null => Self::Null,
        }
    }
}

fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["access_code", "password", "passwd", "token", "auth"]
        .iter()
        .any(|needle| key.contains(needle))
}
