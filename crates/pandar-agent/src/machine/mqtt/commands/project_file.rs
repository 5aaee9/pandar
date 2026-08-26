use pandar_core::PrintCalibrationMode;
use serde::Serialize;

use super::{BambuMqttCommandPayload, next_studio_sequence_id};
use crate::machine::mqtt::commands::payload::{ProjectFilePayload, ProjectFilePayloadPrint};
use pandar_protocol::agent::v1::PrintSubmissionSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileCommand {
    pub printer_model: Option<String>,
    pub filename: String,
    pub url: Option<String>,
    pub md5: Option<String>,
    pub plate_id: u32,
    pub studio_submission_id: u32,
    pub submission_source: PrintSubmissionSource,
    pub task_name: Option<String>,
    pub origin_profile_id: i64,
    pub use_ams: bool,
    pub bed_leveling: bool,
    pub auto_bed_leveling: PrintCalibrationMode,
    pub flow_cali: bool,
    pub vibration_cali: bool,
    pub layer_inspect: bool,
    pub auto_flow_cali: PrintCalibrationMode,
    pub auto_offset_cali: PrintCalibrationMode,
    pub timelapse: bool,
    pub timelapse_use_internal: bool,
    pub bed_type: String,
    pub extruder_cali_manual_mode: Option<i32>,
    pub nozzle_mapping: Vec<i32>,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Vec<ProjectFileAmsMapping2>,
    pub ams_mapping_info: Vec<ProjectFileAmsMappingInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectFileAmsMapping2 {
    pub ams_id: i32,
    pub slot_id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectFileAmsMappingInfo {
    pub ams: i32,
    #[serde(rename = "targetColor")]
    pub target_color: String,
    #[serde(rename = "filamentId")]
    pub filament_id: String,
    #[serde(rename = "filamentType")]
    pub filament_type: String,
    #[serde(rename = "nozzleId", skip_serializing_if = "Option::is_none")]
    pub nozzle_id: Option<i32>,
    #[serde(rename = "sourceColor", skip_serializing_if = "Option::is_none")]
    pub source_color: Option<String>,
}

pub(super) fn project_file_payload(command: &ProjectFileCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    let submission_id = command.studio_submission_id.to_string();
    let profile_id = match command.submission_source {
        PrintSubmissionSource::Studio if command.origin_profile_id > 0 => {
            command.origin_profile_id.to_string()
        }
        PrintSubmissionSource::Studio => submission_id.clone(),
        PrintSubmissionSource::Web => "0".to_owned(),
        PrintSubmissionSource::Unspecified => unreachable!(),
    };
    let payload = ProjectFilePayload {
        print: ProjectFilePayloadPrint {
            command: "project_file",
            sequence_id: sequence_id.clone(),
            param: format!("Metadata/plate_{}.gcode", command.plate_id),
            project_id: submission_id.clone(),
            profile_id,
            task_id: submission_id.clone(),
            subtask_id: submission_id,
            subtask_name: command
                .task_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| project_file_subtask_name(&command.filename)),
            url: command
                .url
                .clone()
                .unwrap_or_else(|| format!("ftp://{}", command.filename)),
            file: command.filename.clone(),
            md5: command.md5.clone().unwrap_or_default(),
            bed_type: command.bed_type.clone(),
            bed_leveling: command.bed_leveling,
            flow_cali: command.flow_cali,
            vibration_cali: command.vibration_cali,
            layer_inspect: command.layer_inspect,
            timelapse: command.timelapse,
            use_ams: command.use_ams,
            ams_mapping: command
                .ams_mapping
                .iter()
                .map(|value| match value {
                    254 | 255 => -1,
                    value => i64::from(*value),
                })
                .collect(),
            ams_mapping2: command.ams_mapping2.clone(),
            nozzle_mapping: nozzle_mapping_for_printer(command),
            ams_mapping_info: (!command.ams_mapping_info.is_empty())
                .then(|| command.ams_mapping_info.clone()),
            auto_bed_leveling: command.auto_bed_leveling.as_u8(),
            nozzle_offset_cali: command.auto_offset_cali.as_u8(),
            cfg: if command.timelapse_use_internal {
                "4".to_owned()
            } else {
                "0".to_owned()
            },
            extrude_cali_flag: command.auto_flow_cali.as_u8(),
            extrude_cali_manual_mode: command.extruder_cali_manual_mode,
        },
    };
    BambuMqttCommandPayload::with_sequence(
        super::super::signing::maybe_sign_project_file_payload(
            payload,
            command.printer_model.as_deref(),
        ),
        sequence_id,
    )
}

fn nozzle_mapping_for_printer(command: &ProjectFileCommand) -> Option<Vec<i32>> {
    (!command.nozzle_mapping.is_empty()).then(|| command.nozzle_mapping.clone())
}

fn project_file_subtask_name(filename: &str) -> String {
    let base = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();
    let stem = base
        .strip_suffix(".gcode.3mf")
        .or_else(|| base.strip_suffix(".3mf"))
        .unwrap_or(base)
        .trim();
    if stem.is_empty() {
        "print".to_string()
    } else {
        stem.to_string()
    }
}
