use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

pub(super) fn json_payload<T: Serialize>(payload: T) -> Value {
    serde_json::to_value(payload).expect("MQTT payload is serializable")
}

#[derive(Serialize)]
pub(super) struct InfoPayload {
    pub(super) info: InfoCommand,
}

#[derive(Serialize)]
pub(super) struct InfoCommand {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct PushingPayload {
    pub(super) pushing: PushingCommand,
}

#[derive(Serialize)]
pub(super) struct PushingCommand {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
    pub(super) version: u8,
    pub(super) push_target: u8,
}

#[derive(Serialize)]
pub(super) struct PrintPayload<T> {
    pub(super) print: T,
}

#[derive(Serialize)]
pub(super) struct BasicPrintCommand {
    pub(super) command: &'static str,
    pub(super) param: &'static str,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct PrintSpeedCommand {
    pub(super) command: &'static str,
    pub(super) param: String,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct SelectExtruderCommand {
    pub(super) command: &'static str,
    pub(super) extruder_index: u32,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct SetNozzleTemperaturePayload {
    pub(super) command: &'static str,
    pub(super) extruder_index: u32,
    pub(super) target_temp: u16,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct GcodeLinePayload {
    pub(super) command: &'static str,
    pub(super) param: String,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct AmsSlotPayload {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
    pub(super) ams_id: u32,
    pub(super) slot_id: u32,
}

#[derive(Serialize)]
pub(super) struct SystemPayload<'a> {
    pub(super) system: ChamberLightPayload<'a>,
}

#[derive(Serialize)]
pub(super) struct ChamberLightPayload<'a> {
    pub(super) command: &'static str,
    pub(super) led_node: &'a str,
    pub(super) led_mode: &'static str,
    pub(super) led_on_time: u16,
    pub(super) led_off_time: u16,
    pub(super) loop_times: u8,
    pub(super) interval_time: u16,
    pub(super) sequence_id: String,
}

#[derive(Serialize)]
pub(super) struct AmsChangeFilamentPayload {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
    pub(super) ams_id: u32,
    pub(super) slot_id: u32,
    pub(super) target: u32,
    pub(super) curr_temp: i16,
    pub(super) tar_temp: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extruder_id: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct ProjectFilePayload {
    pub(super) print: ProjectFilePayloadPrint,
}

#[derive(Serialize)]
pub(super) struct ProjectFilePayloadPrint {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
    pub(super) param: String,
    pub(super) project_id: &'static str,
    pub(super) profile_id: &'static str,
    pub(super) task_id: &'static str,
    pub(super) subtask_id: &'static str,
    pub(super) subtask_name: String,
    pub(super) url: String,
    pub(super) file: String,
    pub(super) md5: String,
    pub(super) bed_type: &'static str,
    pub(super) bed_leveling: bool,
    pub(super) flow_cali: bool,
    pub(super) vibration_cali: bool,
    pub(super) layer_inspect: bool,
    pub(super) timelapse: bool,
    pub(super) use_ams: bool,
    pub(super) ams_mapping: Vec<i64>,
    pub(super) ams_mapping2: Vec<ProjectFileAmsMapping2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ams_mapping_info: Option<Vec<ProjectFileAmsMappingInfo>>,
    pub(super) auto_bed_leveling: u8,
    pub(super) nozzle_offset_cali: u8,
    pub(super) cfg: &'static str,
    pub(super) extrude_cali_flag: u8,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ProjectFileAmsMapping2 {
    pub(super) ams_id: i64,
    pub(super) slot_id: i64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ProjectFileAmsMappingInfo {
    #[serde(rename = "nozzleId")]
    pub(super) nozzle_id: i64,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, ProjectFileAmsMappingInfoExtra>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum ProjectFileAmsMappingInfoExtra {
    Object(BTreeMap<String, ProjectFileAmsMappingInfoExtra>),
    Array(Vec<ProjectFileAmsMappingInfoExtra>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}
