use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

pub(crate) type AmsMapping = Vec<i32>;
pub(crate) type AmsMapping2 = Vec<AmsMapping2Entry>;
pub(crate) type AmsMappingInfo = Vec<AmsMappingInfoEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AmsMapping2Entry {
    pub(crate) ams_id: i32,
    pub(crate) slot_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AmsMappingInfoEntry {
    #[serde(rename = "nozzleId")]
    pub(crate) nozzle_id: i32,
    #[serde(flatten)]
    pub(crate) extra: BTreeMap<String, AmsMappingInfoExtra>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum AmsMappingInfoExtra {
    Object(BTreeMap<String, AmsMappingInfoExtra>),
    Array(Vec<AmsMappingInfoExtra>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

pub(crate) fn validate_mapping_len(len: usize) -> bool {
    len <= 32
}
