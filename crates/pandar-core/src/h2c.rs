use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::StudioFiniteF64;

const MAX_MAPPING_ENTRIES: usize = 32;
const V0_AMS_MAPPING_ENTRIES: usize = 33;
const V0_FILAMENT_INFO_ENTRIES: usize = V0_AMS_MAPPING_ENTRIES * MAX_MAPPING_ENTRIES;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H2cAutoNozzleMappingEnvelope {
    pub print: H2cAutoNozzleMappingRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H2cAutoNozzleMappingRequest {
    pub command: String,
    pub sequence_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extrude_cali_manual_mode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filament_seq: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ams_mapping: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fila_info: Option<Vec<H2cAutoMappingFilamentInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle_info: Option<Vec<H2cAutoMappingNozzleInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_info: Option<Vec<H2cAutoMappingGroupInfo>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H2cAutoMappingGroupInfo {
    pub id: i32,
    pub ext: u8,
    pub dia: f32,
    pub vol: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H2cAutoMappingFilamentInfo {
    pub id: i32,
    pub direction: u8,
    pub group: i32,
    pub nozzle_d: String,
    pub nozzle_v: String,
    pub cate: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H2cAutoMappingNozzleInfo {
    pub pos: i32,
    pub nozzle_d: String,
    pub nozzle_v: String,
    pub wear: f32,
    pub cate: String,
    pub color: String,
}

impl H2cAutoNozzleMappingRequest {
    pub fn is_valid(&self) -> bool {
        if self.command != "get_auto_nozzle_mapping"
            || self.sequence_id.is_empty()
            || self.sequence_id.len() > 20
            || !self.sequence_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return false;
        }

        match self.version {
            Some(1) => self.valid_v1(),
            None | Some(0) => self.valid_v0(),
            Some(_) => false,
        }
    }

    fn valid_v1(&self) -> bool {
        self.calibration.is_none()
            && self.extrude_cali_manual_mode.is_none()
            && self.filament_seq.is_none()
            && self.ams_mapping.is_none()
            && self.fila_info.is_none()
            && self.nozzle_info.is_none()
            && self.group_info.as_ref().is_some_and(|groups| {
                !groups.is_empty()
                    && groups.len() <= MAX_MAPPING_ENTRIES
                    && groups.iter().all(H2cAutoMappingGroupInfo::is_valid)
                    && all_unique(groups.iter().map(|group| group.id))
            })
    }

    fn valid_v0(&self) -> bool {
        self.group_info.is_none()
            && self
                .calibration
                .is_some_and(|value| (0..=2).contains(&value))
            && self
                .extrude_cali_manual_mode
                .is_some_and(|value| (-1..=1).contains(&value))
            && self.filament_seq.as_ref().is_some_and(|values| {
                !values.is_empty()
                    && values.len() <= V0_AMS_MAPPING_ENTRIES
                    && values.iter().all(|value| *value >= -1)
            })
            && self.ams_mapping.as_ref().is_some_and(|values| {
                values.len() == V0_AMS_MAPPING_ENTRIES
                    && values.iter().all(|value| (0..=0xffff).contains(value))
            })
            && self.fila_info.as_ref().is_some_and(|values| {
                values.len() <= V0_FILAMENT_INFO_ENTRIES
                    && values.iter().all(H2cAutoMappingFilamentInfo::is_valid)
            })
            && self.nozzle_info.as_ref().is_some_and(|values| {
                values.len() <= 8 && values.iter().all(H2cAutoMappingNozzleInfo::is_valid)
            })
    }
}

impl H2cAutoMappingGroupInfo {
    fn is_valid(&self) -> bool {
        (0..MAX_MAPPING_ENTRIES as i32).contains(&self.id)
            && (1..=2).contains(&self.ext)
            && valid_diameter(self.dia)
            && valid_flow(&self.vol)
    }
}

impl H2cAutoMappingFilamentInfo {
    fn is_valid(&self) -> bool {
        (1..=V0_AMS_MAPPING_ENTRIES as i32).contains(&self.id)
            && (1..=2).contains(&self.direction)
            && (0..MAX_MAPPING_ENTRIES as i32).contains(&self.group)
            && valid_diameter_text(&self.nozzle_d)
            && valid_flow(&self.nozzle_v)
            && self.cate.len() <= 128
            && self.color.len() <= 16
    }
}

impl H2cAutoMappingNozzleInfo {
    fn is_valid(&self) -> bool {
        valid_physical_nozzle_id(self.pos)
            && valid_diameter_text(&self.nozzle_d)
            && valid_flow(&self.nozzle_v)
            && self.wear.is_finite()
            && self.wear >= 0.0
            && self.cate.len() <= 128
            && self.color.len() <= 16
    }
}

fn all_unique(values: impl IntoIterator<Item = i32>) -> bool {
    let mut seen = Vec::new();
    values.into_iter().all(|value| {
        if seen.contains(&value) {
            false
        } else {
            seen.push(value);
            true
        }
    })
}

fn valid_diameter(value: f32) -> bool {
    value.is_finite() && [0.2_f32, 0.4, 0.6, 0.8].contains(&value)
}

fn valid_diameter_text(value: &str) -> bool {
    value.parse::<f32>().is_ok_and(valid_diameter)
}

fn valid_flow(value: &str) -> bool {
    matches!(
        value,
        "Standard" | "High Flow" | "TPU Flow" | "TPU High Flow" | "E3D High Flow"
    )
}

pub fn valid_physical_nozzle_id(value: i32) -> bool {
    matches!(value, 0 | 1 | 16..=21)
}

pub fn valid_h2c_nozzle_mapping(mapping: &[i32]) -> bool {
    !mapping.is_empty()
        && mapping.len() <= MAX_MAPPING_ENTRIES
        && mapping
            .iter()
            .all(|value| *value == -1 || valid_physical_nozzle_id(*value))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H2cAutoNozzleMappingResponseEnvelope {
    pub print: H2cAutoNozzleMappingResponse,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct H2cAutoNozzleMappingResponse {
    pub command: String,
    pub sequence_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errno: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping: Option<Vec<i32>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn response_version(value: Option<&Value>) -> Option<u8> {
    match value {
        None => Some(0),
        Some(Value::Number(value)) => value.as_u64().and_then(|value| u8::try_from(value).ok()),
        Some(_) => None,
    }
}

impl H2cAutoNozzleMappingResponseEnvelope {
    pub fn is_valid_for(&self, request: &H2cAutoNozzleMappingRequest) -> bool {
        let response = &self.print;
        if response.command != "get_auto_nozzle_mapping"
            || response.sequence_id != request.sequence_id
        {
            return false;
        }
        match response.result.as_deref() {
            Some("fail" | "failed") => true,
            Some("success") => {
                response_version(response.version.as_ref())
                    == Some(request.version.unwrap_or_default())
                    && response
                        .mapping
                        .as_deref()
                        .is_some_and(valid_h2c_nozzle_mapping)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BambuNozzleSystem {
    pub nozzle: BambuNozzleDevice,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<BambuNozzleHolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BambuNozzleDevice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exist: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tar_id: Option<i32>,
    pub info: Vec<BambuNozzleInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BambuNozzleInfo {
    pub id: i32,
    pub diameter: StudioFiniteF64,
    #[serde(rename = "type")]
    pub nozzle_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fila_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear: Option<StudioFiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p_t: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_m: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BambuNozzleHolder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<i32>,
}
