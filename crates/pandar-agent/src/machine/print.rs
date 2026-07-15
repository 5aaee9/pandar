use anyhow::{Context, bail};
use pandar_core::PrintCalibrationMode;

use crate::{
    machine::{
        BambuMqttTransport, BambuPrinterEndpoint, MachineFileTransfer, MachineJsonPayload,
        PrintProjectDispatchResult, TransferModeCache,
        brtc::md5_upper,
        compatibility::{
            auto_bed_leveling_supported, auto_flow_calibration_supported,
            flow_calibration_supported, nozzle_offset_calibration_supported,
        },
        file_transfer::run_with_transfer_mode,
        mqtt::{BambuMqttCommand, BambuMqttTopics, ProjectFileCommand, PublishedMqttCommand},
    },
    protocol::agent::v1::PrintProjectFile,
};

pub async fn dispatch_print_project_file<F, T>(
    endpoint: &BambuPrinterEndpoint,
    transfer: &F,
    mqtt: &T,
    cache: &TransferModeCache,
    command: &PrintProjectFile,
    artifact: &[u8],
) -> anyhow::Result<PrintProjectDispatchResult>
where
    F: MachineFileTransfer + Send + Sync,
    T: BambuMqttTransport + Send + Sync,
{
    let auto_bed_leveling =
        required_calibration_mode(command.auto_bed_leveling, "auto_bed_leveling")?;
    let auto_flow_cali = required_calibration_mode(command.auto_flow_cali, "auto_flow_cali")?;
    let auto_offset_cali = required_calibration_mode(command.auto_offset_cali, "auto_offset_cali")?;

    if (command.flow_cali || auto_flow_cali == PrintCalibrationMode::On)
        && !flow_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "flow calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if auto_flow_cali == PrintCalibrationMode::Auto
        && !auto_flow_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "automatic flow calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if auto_bed_leveling == PrintCalibrationMode::Auto
        && !auto_bed_leveling_supported(endpoint.model.as_deref())
    {
        bail!(
            "automatic bed leveling is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if auto_offset_cali != PrintCalibrationMode::Off
        && !nozzle_offset_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "nozzle offset calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }

    let remote_path = pick_remote_name(&command.filename);
    let uploaded = run_with_transfer_mode(endpoint, cache, false, |mode| {
        let remote_path = remote_path.clone();
        async move { transfer.upload(&remote_path, artifact, mode).await }
    })
    .await
    .with_context(|| format!("upload print artifact to {}", endpoint.serial))?;

    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    let md5 = md5_upper(artifact);
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: endpoint.model.clone(),
        filename: uploaded.path.clone(),
        url: Some(uploaded.url.clone()),
        md5: Some(md5.clone()),
        plate_id: command.plate_id,
        task_id: command.job_id.clone(),
        subtask_id: command.artifact_id.clone(),
        use_ams: command.use_ams,
        bed_leveling: command.bed_leveling,
        auto_bed_leveling,
        flow_cali: command.flow_cali,
        auto_flow_cali,
        auto_offset_cali,
        timelapse: command.timelapse,
        ams_mapping_json: non_empty_string(&command.ams_mapping_json),
        ams_mapping2_json: non_empty_string(&command.ams_mapping2_json),
        ams_mapping_info_json: non_empty_string(&command.ams_mapping_info_json),
    })
    .payload();
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request.clone(),
        payload: payload.clone(),
        qos: 0,
    })
    .await
    .with_context(|| format!("publish project_file to {}", endpoint.serial))?;

    Ok(PrintProjectDispatchResult {
        topic: topics.request,
        payload: MachineJsonPayload::from(payload),
        qos: 0,
        uploaded_path: uploaded.path,
        uploaded_url: uploaded.url,
        md5,
    })
}
fn required_calibration_mode(
    value: Option<i32>,
    field: &str,
) -> anyhow::Result<PrintCalibrationMode> {
    let value = value.ok_or_else(|| anyhow::anyhow!("missing {field}"))?;
    let value = u8::try_from(value).with_context(|| format!("invalid {field} calibration mode"))?;
    PrintCalibrationMode::try_from(value)
        .with_context(|| format!("invalid {field} calibration mode"))
}

pub(crate) fn pick_remote_name(filename: &str) -> String {
    let base = filename
        .rsplit(['/', '\\'])
        .next()
        .map(sanitize_remote_name)
        .unwrap_or_default();
    let stripped = strip_3mf_extension(base);
    if stripped.is_empty() {
        "print.gcode.3mf".to_owned()
    } else {
        format!("{stripped}.gcode.3mf")
    }
}

fn sanitize_remote_name(name: &str) -> String {
    name.trim_start_matches(['.', '/', '\\', ' '])
        .trim_end_matches([' ', '.'])
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '/' || ch == '\\' {
                '_'
            } else {
                ch
            }
        })
        .collect()
}

fn strip_3mf_extension(mut name: String) -> String {
    if name.ends_with(".gcode.3mf") {
        name.truncate(name.len() - ".gcode.3mf".len());
    } else if name.ends_with(".3mf") {
        name.truncate(name.len() - ".3mf".len());
    }
    name
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}
