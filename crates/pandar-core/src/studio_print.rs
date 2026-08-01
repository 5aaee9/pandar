use serde::{Deserialize, Serialize};

use crate::{CoreError, PrintCalibrationMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct StudioSubmissionId(i32);

impl StudioSubmissionId {
    pub fn get(self) -> i32 {
        self.0
    }
}

impl TryFrom<i64> for StudioSubmissionId {
    type Error = CoreError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        let value =
            i32::try_from(value).map_err(|_| CoreError::InvalidStudioSubmissionId(value))?;
        if value <= 0 {
            return Err(CoreError::InvalidStudioSubmissionId(i64::from(value)));
        }
        Ok(Self(value))
    }
}

impl From<StudioSubmissionId> for i64 {
    fn from(value: StudioSubmissionId) -> Self {
        i64::from(value.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct StudioFiniteF64(f64);

impl Eq for StudioFiniteF64 {}

impl StudioFiniteF64 {
    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for StudioFiniteF64 {
    type Error = CoreError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        value
            .is_finite()
            .then_some(Self(value))
            .ok_or(CoreError::NonFiniteStudioNumber)
    }
}

impl From<StudioFiniteF64> for f64 {
    fn from(value: StudioFiniteF64) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum StudioPrintMetadata {
    #[serde(rename = "1")]
    V1(StudioPrintMetadataV1),
}

impl StudioPrintMetadata {
    pub fn nozzle_mapping(&self) -> &[i32] {
        match self {
            Self::V1(metadata) => &metadata.nozzle_mapping,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioPrintMetadataV1 {
    pub task_name: String,
    pub project_name: String,
    pub preset_name: String,
    pub config_plate_index: Option<u32>,
    pub nozzle_mapping: Vec<i32>,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Vec<StudioAmsMappingEntry>,
    pub ams_mapping_info: Vec<StudioAmsMappingInfo>,
    pub nozzles_info: Vec<StudioNozzleInfo>,
    pub connection_type: String,
    pub comments: String,
    pub origin_profile_id: i64,
    pub stl_design_id: i64,
    pub origin_model_id: String,
    pub print_type: String,
    pub submitted_device_name: String,
    pub task_bed_leveling: bool,
    pub task_flow_cali: bool,
    pub task_vibration_cali: bool,
    pub task_layer_inspect: bool,
    pub task_record_timelapse: bool,
    pub task_timelapse_use_internal: bool,
    pub task_use_ams: bool,
    pub task_bed_type: String,
    pub auto_bed_leveling: PrintCalibrationMode,
    pub auto_flow_cali: PrintCalibrationMode,
    pub auto_offset_cali: PrintCalibrationMode,
    pub extruder_cali_manual_mode: i8,
    pub try_emmc_print: bool,
    pub svc_context: String,
    pub slicer_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioAmsMappingEntry {
    pub ams_id: i32,
    pub slot_id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioAmsMappingInfo {
    pub ams: i32,
    #[serde(rename = "targetColor")]
    pub target_color: String,
    #[serde(rename = "filamentId")]
    pub filament_id: String,
    #[serde(rename = "filamentType")]
    pub filament_type: String,
    #[serde(rename = "nozzleId")]
    pub nozzle_id: Option<i32>,
    #[serde(rename = "sourceColor")]
    pub source_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioNozzleInfo {
    pub id: i32,
    #[serde(rename = "type")]
    pub nozzle_type: Option<String>,
    #[serde(rename = "flowSize")]
    pub flow_size: Option<String>,
    pub diameter: Option<StudioFiniteF64>,
}
