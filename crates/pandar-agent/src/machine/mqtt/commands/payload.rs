use serde::Serialize;
use serde_json::Value;

use super::{ProjectFileAmsMapping2, ProjectFileAmsMappingInfo};

pub(in crate::machine::mqtt) fn json_payload<T: Serialize>(payload: T) -> Value {
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
pub(super) struct PrintErrorCommand<'a> {
    pub(super) command: &'static str,
    pub(super) err: String,
    pub(super) job_id: &'a str,
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
pub(super) struct AmsFilamentDryingPayload<'a> {
    pub(super) command: &'static str,
    pub(super) sequence_id: String,
    pub(super) ams_id: u32,
    pub(super) mode: u8,
    pub(super) filament: &'a str,
    pub(super) temp: u16,
    pub(super) duration: u16,
    pub(super) humidity: u8,
    pub(super) rotate_tray: bool,
    pub(super) cooling_temp: u16,
    pub(super) close_power_conflict: bool,
}

#[derive(Clone, Serialize)]
pub(in crate::machine::mqtt) struct ProjectFilePayload {
    pub(in crate::machine::mqtt) print: ProjectFilePayloadPrint,
}

#[derive(Clone, Serialize)]
pub(in crate::machine::mqtt) struct ProjectFilePayloadPrint {
    pub(in crate::machine::mqtt) command: &'static str,
    pub(in crate::machine::mqtt) sequence_id: String,
    pub(in crate::machine::mqtt) param: String,
    pub(in crate::machine::mqtt) project_id: String,
    pub(in crate::machine::mqtt) profile_id: String,
    pub(in crate::machine::mqtt) task_id: String,
    pub(in crate::machine::mqtt) subtask_id: String,
    pub(in crate::machine::mqtt) subtask_name: String,
    pub(in crate::machine::mqtt) url: String,
    pub(in crate::machine::mqtt) file: String,
    pub(in crate::machine::mqtt) md5: String,
    pub(in crate::machine::mqtt) bed_type: String,
    pub(in crate::machine::mqtt) bed_leveling: bool,
    pub(in crate::machine::mqtt) flow_cali: bool,
    pub(in crate::machine::mqtt) vibration_cali: bool,
    pub(in crate::machine::mqtt) layer_inspect: bool,
    pub(in crate::machine::mqtt) timelapse: bool,
    pub(in crate::machine::mqtt) use_ams: bool,
    pub(in crate::machine::mqtt) ams_mapping: Vec<i64>,
    pub(in crate::machine::mqtt) ams_mapping2: Vec<ProjectFileAmsMapping2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::machine::mqtt) nozzle_mapping: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::machine::mqtt) ams_mapping_info: Option<Vec<ProjectFileAmsMappingInfo>>,
    pub(in crate::machine::mqtt) auto_bed_leveling: u8,
    pub(in crate::machine::mqtt) nozzle_offset_cali: u8,
    pub(in crate::machine::mqtt) cfg: String,
    pub(in crate::machine::mqtt) extrude_cali_flag: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::machine::mqtt) extrude_cali_manual_mode: Option<i32>,
}
