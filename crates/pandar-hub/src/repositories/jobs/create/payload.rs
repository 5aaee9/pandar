use pandar_core::JobId;

use crate::repositories::{CreatePrintJob, PrintProjectFilePayload};

use super::NewPrintJobFromArtifact;

pub(super) fn payload(
    input: &CreatePrintJob,
    job_id: JobId,
    serial_number: &str,
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
        flow_cali: input.flow_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
    }
}

pub(super) fn payload_from_existing_artifact(
    input: &NewPrintJobFromArtifact,
    job_id: JobId,
    serial_number: &str,
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
        flow_cali: input.flow_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
    }
}

fn artifact_download_path(agent_id: &pandar_core::AgentId, artifact_id: &str) -> String {
    format!("/api/v1/agents/{agent_id}/artifacts/{artifact_id}")
}
