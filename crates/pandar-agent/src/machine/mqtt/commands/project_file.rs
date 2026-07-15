use super::{BambuMqttCommandPayload, ProjectFileCommand, next_studio_sequence_id};
use crate::machine::mqtt::commands::payload::{
    ProjectFileAmsMapping2, ProjectFileAmsMappingInfo, ProjectFilePayload, ProjectFilePayloadPrint,
};

pub(super) fn project_file_payload(command: &ProjectFileCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    let submission_id = project_file_submission_id();
    let payload = ProjectFilePayload {
        print: ProjectFilePayloadPrint {
            command: "project_file",
            sequence_id: sequence_id.clone(),
            param: format!("Metadata/plate_{}.gcode", command.plate_id),
            project_id: submission_id.clone(),
            profile_id: "0",
            task_id: submission_id.clone(),
            subtask_id: submission_id,
            subtask_name: project_file_subtask_name(&command.filename),
            url: command
                .url
                .clone()
                .unwrap_or_else(|| format!("ftp://{}", command.filename)),
            file: command.filename.clone(),
            md5: command.md5.clone().unwrap_or_default(),
            bed_type: "auto",
            bed_leveling: command.bed_leveling,
            flow_cali: command.flow_cali,
            vibration_cali: false,
            layer_inspect: false,
            timelapse: command.timelapse,
            use_ams: command.use_ams,
            ams_mapping: command
                .ams_mapping_json
                .as_deref()
                .and_then(project_file_ams_mapping)
                .unwrap_or_default(),
            ams_mapping2: command
                .ams_mapping2_json
                .as_deref()
                .and_then(project_file_ams_mapping2)
                .unwrap_or_default(),
            ams_mapping_info: command
                .ams_mapping_info_json
                .as_deref()
                .and_then(project_file_ams_mapping_info),
            auto_bed_leveling: command.auto_bed_leveling.as_u8(),
            nozzle_offset_cali: command.auto_offset_cali.as_u8(),
            cfg: "0",
            extrude_cali_flag: command.auto_flow_cali.as_u8(),
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

fn project_file_submission_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(1);
    let id = (millis % 2_147_483_647).max(1);
    id.to_string()
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

fn project_file_ams_mapping(raw: &str) -> Option<Vec<i64>> {
    Some(
        serde_json::from_str::<Vec<i64>>(raw)
            .ok()?
            .into_iter()
            .map(|value| match value {
                254 | 255 => -1,
                _ => value,
            })
            .collect(),
    )
}

fn project_file_ams_mapping2(raw: &str) -> Option<Vec<ProjectFileAmsMapping2>> {
    serde_json::from_str(raw).ok()
}

fn project_file_ams_mapping_info(raw: &str) -> Option<Vec<ProjectFileAmsMappingInfo>> {
    serde_json::from_str(raw).ok()
}
