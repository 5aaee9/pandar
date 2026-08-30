use anyhow::{Context, bail};
use pandar_core::PrintCalibrationMode;

use crate::machine::{
    BambuMqttTransport, BambuPrinterEndpoint, MachineFileTransfer, MachineJsonPayload,
    PrintProjectDispatchResult,
    brtc::md5_upper,
    compatibility::{
        auto_bed_leveling_supported, auto_flow_calibration_supported, flow_calibration_supported,
        nozzle_offset_calibration_supported,
    },
    file_transfer::PrintUploadPolicy,
    mqtt::{
        BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, ProjectFileAmsMapping2,
        ProjectFileAmsMappingInfo, ProjectFileCommand, PublishedMqttCommand,
    },
};
use pandar_protocol::agent::v1::{
    PrintProjectFile, PrintProjectFileOptions, PrintSubmissionSource, StudioTaskMetadata,
};

const MAX_STUDIO_MAPPING_ENTRIES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StudioBedType {
    SuperTack,
    Cool,
    Engineering,
    SmoothPei,
    TexturedPei,
}

impl StudioBedType {
    fn as_str(self) -> &'static str {
        match self {
            Self::SuperTack => "supertack_plate",
            Self::Cool => "cool_plate",
            Self::Engineering => "eng_plate",
            Self::SmoothPei => "hot_plate",
            Self::TexturedPei => "textured_plate",
        }
    }
}

impl TryFrom<&str> for StudioBedType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "supertack_plate" => Ok(Self::SuperTack),
            "cool_plate" => Ok(Self::Cool),
            "eng_plate" => Ok(Self::Engineering),
            "hot_plate" => Ok(Self::SmoothPei),
            "textured_plate" => Ok(Self::TexturedPei),
            _ => bail!("invalid Studio bed_type {value:?}"),
        }
    }
}

pub(crate) struct ValidatedPrintProjectFile<'a> {
    options: &'a PrintProjectFileOptions,
    submission_source: PrintSubmissionSource,
    task_metadata: Option<&'a StudioTaskMetadata>,
    bed_type: String,
    auto_bed_leveling: PrintCalibrationMode,
    auto_flow_cali: PrintCalibrationMode,
    auto_offset_cali: PrintCalibrationMode,
    extruder_cali_manual_mode: Option<i32>,
}

pub(crate) fn validate_print_project_file_command(
    command: &PrintProjectFile,
) -> anyhow::Result<ValidatedPrintProjectFile<'_>> {
    if command.artifact_download_path.trim().is_empty() {
        bail!("missing artifact_download_path");
    }
    if command.plate_id == 0 || command.plate_id > i32::MAX as u32 {
        bail!("invalid plate_id; expected 1..={}", i32::MAX);
    }
    if command.studio_submission_id == 0 || command.studio_submission_id > i32::MAX as u32 {
        bail!("invalid studio_submission_id; expected 1..={}", i32::MAX);
    }
    let submission_source = PrintSubmissionSource::try_from(command.submission_source)
        .context("invalid print submission source")?;
    if submission_source == PrintSubmissionSource::Unspecified {
        bail!("missing print submission source");
    }
    let options = command
        .options
        .as_ref()
        .context("missing print project file options")?;
    let (
        task_metadata,
        bed_type,
        auto_bed_leveling,
        auto_flow_cali,
        auto_offset_cali,
        extruder_cali_manual_mode,
    ) = match submission_source {
        PrintSubmissionSource::Studio => {
            let task_metadata = command
                .task_metadata
                .as_ref()
                .context("missing Studio task metadata")?;
            let bed_type = StudioBedType::try_from(options.bed_type.as_str())?;
            let extruder_cali_manual_mode = options
                .extruder_cali_manual_mode
                .context("missing extruder_cali_manual_mode")?;
            if !(-1..=1).contains(&extruder_cali_manual_mode) {
                bail!("invalid extruder_cali_manual_mode; expected -1, 0, or 1");
            }
            (
                Some(task_metadata),
                bed_type.as_str().to_owned(),
                required_calibration_mode(options.auto_bed_leveling, "auto_bed_leveling")?,
                required_calibration_mode(options.auto_flow_cali, "auto_flow_cali")?,
                required_calibration_mode(options.auto_offset_cali, "auto_offset_cali")?,
                Some(extruder_cali_manual_mode),
            )
        }
        PrintSubmissionSource::Web => {
            if command.task_metadata.is_some() {
                bail!("Web print command must not contain Studio task metadata");
            }
            if options.extruder_cali_manual_mode.is_some() {
                bail!("Web print command must not contain extruder_cali_manual_mode");
            }
            (
                None,
                if options.bed_type.trim().is_empty() {
                    "auto".to_owned()
                } else {
                    options.bed_type.clone()
                },
                required_calibration_mode(options.auto_bed_leveling, "auto_bed_leveling")?,
                required_calibration_mode(options.auto_flow_cali, "auto_flow_cali")?,
                required_calibration_mode(options.auto_offset_cali, "auto_offset_cali")?,
                None,
            )
        }
        PrintSubmissionSource::Unspecified => unreachable!(),
    };
    for (field, len) in [
        ("nozzle_mapping", options.nozzle_mapping.len()),
        ("ams_mapping", options.ams_mapping.len()),
        ("ams_mapping2", options.ams_mapping2.len()),
        ("ams_mapping_info", options.ams_mapping_info.len()),
    ] {
        if len > MAX_STUDIO_MAPPING_ENTRIES {
            bail!("invalid {field}; expected at most {MAX_STUDIO_MAPPING_ENTRIES} entries");
        }
    }
    if options
        .nozzles_info
        .iter()
        .filter_map(|nozzle| nozzle.diameter)
        .any(|diameter| !diameter.is_finite())
    {
        bail!("invalid nozzles_info diameter; expected a finite number");
    }

    Ok(ValidatedPrintProjectFile {
        options,
        submission_source,
        task_metadata,
        bed_type,
        auto_bed_leveling,
        auto_flow_cali,
        auto_offset_cali,
        extruder_cali_manual_mode,
    })
}

pub async fn dispatch_print_project_file<F, T>(
    endpoint: &BambuPrinterEndpoint,
    transfer: &F,
    mqtt: &T,
    command: &PrintProjectFile,
    artifact: &[u8],
) -> anyhow::Result<PrintProjectDispatchResult>
where
    F: MachineFileTransfer + Send + Sync,
    T: BambuMqttTransport + Send + Sync,
{
    let validated = validate_print_project_file_command(command)
        .context("validate print project file command")?;
    let options = validated.options;

    if (options.flow_cali || validated.auto_flow_cali == PrintCalibrationMode::On)
        && !flow_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "flow calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if validated.auto_flow_cali == PrintCalibrationMode::Auto
        && !auto_flow_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "automatic flow calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if validated.auto_bed_leveling == PrintCalibrationMode::Auto
        && !auto_bed_leveling_supported(endpoint.model.as_deref())
    {
        bail!(
            "automatic bed leveling is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }
    if validated.auto_offset_cali != PrintCalibrationMode::Off
        && !nozzle_offset_calibration_supported(endpoint.model.as_deref())
    {
        bail!(
            "nozzle offset calibration is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }

    let remote_path = pick_remote_name(&command.filename);
    let print_upload_policy = PrintUploadPolicy {
        try_emmc_print: options.try_emmc_print,
    };
    let uploaded = transfer
        .upload_print(&remote_path, artifact, print_upload_policy)
        .await
        .with_context(|| {
            format!(
                "upload print artifact to {} at {}",
                endpoint.serial, endpoint.host
            )
        })?;

    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    let md5 = md5_upper(artifact);
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        printer_model: endpoint.model.clone(),
        filename: uploaded.path.clone(),
        url: Some(uploaded.url.clone()),
        md5: Some(md5.clone()),
        plate_id: command.plate_id,
        studio_submission_id: command.studio_submission_id,
        submission_source: validated.submission_source,
        task_name: validated
            .task_metadata
            .map(|metadata| metadata.task_name.clone()),
        origin_profile_id: validated
            .task_metadata
            .map_or(0, |metadata| metadata.origin_profile_id),
        use_ams: options.use_ams,
        bed_leveling: options.bed_leveling,
        auto_bed_leveling: validated.auto_bed_leveling,
        flow_cali: options.flow_cali,
        vibration_cali: options.vibration_cali,
        layer_inspect: options.layer_inspect,
        auto_flow_cali: validated.auto_flow_cali,
        auto_offset_cali: validated.auto_offset_cali,
        timelapse: options.record_timelapse,
        timelapse_use_internal: options.timelapse_use_internal,
        bed_type: validated.bed_type,
        extruder_cali_manual_mode: validated.extruder_cali_manual_mode,
        nozzle_mapping: options.nozzle_mapping.clone(),
        ams_mapping: options.ams_mapping.clone(),
        ams_mapping2: options
            .ams_mapping2
            .iter()
            .map(|entry| ProjectFileAmsMapping2 {
                ams_id: entry.ams_id,
                slot_id: entry.slot_id,
            })
            .collect(),
        ams_mapping_info: options
            .ams_mapping_info
            .iter()
            .map(|entry| ProjectFileAmsMappingInfo {
                ams: entry.ams,
                target_color: entry.target_color.clone(),
                filament_id: entry.filament_id.clone(),
                filament_type: entry.filament_type.clone(),
                nozzle_id: entry.nozzle_id,
                source_color: entry.source_color.clone(),
            })
            .collect(),
    })
    .payload();
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request.clone(),
        payload: payload.clone(),
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .with_context(|| format!("publish project_file to {}", endpoint.serial))?;

    Ok(PrintProjectDispatchResult {
        topic: topics.request,
        payload: MachineJsonPayload::from(payload),
        qos: BAMBU_MQTT_QOS,
        uploaded_path: uploaded.path,
        uploaded_url: uploaded.url,
        md5,
    })
}
fn required_calibration_mode(
    value: Option<i32>,
    field: &str,
) -> anyhow::Result<PrintCalibrationMode> {
    let value = value.with_context(|| format!("missing {field} calibration mode"))?;
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
