use anyhow::{Context, bail};

use crate::{
    machine::{
        BambuMqttTransport, BambuPrinterEndpoint, MachineFileTransfer, TransferModeCache,
        brtc::md5_upper,
        compatibility::flow_calibration_supported,
        file_transfer::run_with_transfer_mode,
        mqtt::{
            BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, ProjectFileCommand,
            PublishedMqttCommand,
        },
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
) -> anyhow::Result<()>
where
    F: MachineFileTransfer + Send + Sync,
    T: BambuMqttTransport + Send + Sync,
{
    if command.flow_cali && !flow_calibration_supported(endpoint.model.as_deref()) {
        bail!(
            "flow calibration is not supported for model {}",
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
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request,
        payload: BambuMqttCommand::ProjectFile(ProjectFileCommand {
            filename: uploaded.path,
            url: Some(uploaded.url),
            md5: Some(md5_upper(artifact)),
            plate_id: command.plate_id,
            task_id: command.job_id.clone(),
            subtask_id: command.artifact_id.clone(),
            use_ams: command.use_ams,
            flow_cali: command.flow_cali,
            timelapse: command.timelapse,
            ams_mapping_json: non_empty_string(&command.ams_mapping_json),
            ams_mapping2_json: non_empty_string(&command.ams_mapping2_json),
        })
        .payload(),
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .with_context(|| format!("publish project_file to {}", endpoint.serial))
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
