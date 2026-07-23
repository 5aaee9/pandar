use pandar_core::{JobId, StudioPrintMetadata, StudioSubmissionId};

use crate::repositories::{CreatePrintJob, PrintProjectFilePayload};

use super::NewPrintJobFromArtifact;

pub(super) fn payload(
    input: &CreatePrintJob,
    job_id: JobId,
    serial_number: &str,
    studio_submission_id: StudioSubmissionId,
    studio_metadata: Option<StudioPrintMetadata>,
) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: job_id.to_string(),
        artifact_id: input.artifact_id.clone(),
        printer_id: input.printer_id.clone(),
        serial_number: serial_number.to_string(),
        filename: input.artifact_filename.clone(),
        storage_path: input.artifact_storage_path.clone(),
        artifact_download_path: artifact_download_path(&input.agent_id, &input.artifact_id),
        size_bytes: input.artifact_size_bytes,
        plate_id: input.plate_id,
        use_ams: input.use_ams,
        auto_bed_leveling: input.auto_bed_leveling,
        bed_leveling: input.bed_leveling,
        flow_cali: input.flow_cali,
        auto_flow_cali: input.auto_flow_cali,
        auto_offset_cali: input.auto_offset_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
        ams_mapping_info_json: input.ams_mapping_info_json.clone(),
        studio_submission_id,
        studio_metadata,
    }
}

pub(super) fn payload_from_existing_artifact(
    input: &NewPrintJobFromArtifact,
    job_id: JobId,
    serial_number: &str,
    studio_submission_id: StudioSubmissionId,
    studio_metadata: Option<StudioPrintMetadata>,
) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: job_id.to_string(),
        artifact_id: input.artifact_id.clone(),
        printer_id: input.printer_id.clone(),
        serial_number: serial_number.to_string(),
        filename: input.artifact_filename.clone(),
        storage_path: input.artifact_storage_path.clone(),
        artifact_download_path: artifact_download_path(&input.agent_id, &input.artifact_id),
        size_bytes: input.artifact_size_bytes,
        plate_id: input.plate_id,
        use_ams: input.use_ams,
        auto_bed_leveling: input.auto_bed_leveling,
        auto_flow_cali: input.auto_flow_cali,
        bed_leveling: input.bed_leveling,
        flow_cali: input.flow_cali,
        auto_offset_cali: input.auto_offset_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
        ams_mapping_info_json: input.ams_mapping_info_json.clone(),
        studio_submission_id,
        studio_metadata,
    }
}

fn artifact_download_path(agent_id: &pandar_core::AgentId, artifact_id: &str) -> String {
    format!("/api/v1/agents/{agent_id}/artifacts/{artifact_id}")
}
