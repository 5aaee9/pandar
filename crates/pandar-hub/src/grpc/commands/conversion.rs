use pandar_core::{CommandId, CommandRecord, StudioPrintMetadata};
use tonic::Status;

mod operations;

use operations::proto_printer_operation;

use crate::{
    protocol::agent::v1::{
        DiagnosePrinter, DiscoverPrinters, HubCommand, PrintProjectFile, PrintProjectFileOptions,
        PrintSubmissionSource, PrinterOperation, RefreshPrinterMaterials, RefreshPrinters,
        ReloadPrinterConnection, StudioAmsMappingEntry, StudioAmsMappingInfo, StudioNozzleInfo,
        StudioTaskMetadata, hub_command,
    },
    repositories::{
        DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload,
        PrinterOperationKind, PrinterOperationPayload, RefreshPrinterMaterialsPayload,
        ReloadPrinterConnectionPayload,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandConversionOptions {
    pub require_artifact_download_path: bool,
}

pub(super) fn persisted_printer_operation_payload(
    command: &CommandRecord,
) -> Result<Option<PrinterOperationPayload>, serde_json::Error> {
    if command.kind != "printer_operation" {
        return Ok(None);
    }
    serde_json::from_str(&command.payload_json).map(Some)
}

fn invalid_printer_operation_payload_status(
    command: &CommandRecord,
    err: serde_json::Error,
) -> Status {
    tracing::error!(
        command_id = %command.id,
        error = %format!("{err:#}"),
        "failed to deserialize printer operation command payload"
    );
    Status::internal("invalid printer operation command payload")
}

pub fn hub_command_from_record(command: CommandRecord) -> Result<HubCommand, Status> {
    hub_command_from_record_with_options(
        command,
        CommandConversionOptions {
            require_artifact_download_path: false,
        },
    )
}

pub fn hub_command_from_record_with_options(
    command: CommandRecord,
    options: CommandConversionOptions,
) -> Result<HubCommand, Status> {
    let command_id = command.id.to_string();
    let command = match command.kind.as_str() {
        "refresh_printers" => hub_command::Command::RefreshPrinters(RefreshPrinters {}),
        "discover_printers" => {
            let payload: DiscoverPrintersPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize discover printers command payload"
                    );
                    Status::internal("invalid discover printers command payload")
                })?;
            hub_command::Command::DiscoverPrinters(DiscoverPrinters {
                timeout_seconds: payload.timeout_seconds,
            })
        }
        "diagnose_printer" => {
            let payload: DiagnosePrinterPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize diagnose printer command payload"
                    );
                    Status::internal("invalid diagnose printer command payload")
                })?;
            hub_command::Command::DiagnosePrinter(DiagnosePrinter {
                serial_number: payload.serial_number,
            })
        }
        "refresh_printer_materials" => {
            let payload: RefreshPrinterMaterialsPayload =
                serde_json::from_str(&command.payload_json).map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize refresh printer materials command payload"
                    );
                    Status::internal("invalid refresh printer materials command payload")
                })?;
            hub_command::Command::RefreshPrinterMaterials(RefreshPrinterMaterials {
                printer_id: payload.printer_id,
                serial_number: payload.serial_number,
            })
        }
        "reload_printer_connection" => {
            let payload: ReloadPrinterConnectionPayload =
                serde_json::from_str(&command.payload_json).map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize reload printer connection command payload"
                    );
                    Status::internal("invalid reload printer connection command payload")
                })?;
            hub_command::Command::ReloadPrinterConnection(ReloadPrinterConnection {
                printer_id: payload.printer_id,
                serial_number: payload.serial_number,
            })
        }
        "printer_operation" => {
            let payload = persisted_printer_operation_payload(&command)
                .map_err(|err| invalid_printer_operation_payload_status(&command, err))?
                .expect("printer operation kind checked above");
            if matches!(
                &payload.operation,
                PrinterOperationKind::HandlePrintError { .. }
                    | PrinterOperationKind::GetAutoNozzleMapping { .. }
            ) {
                tracing::error!(
                    command_id = %command.id,
                    command_kind = %command.kind,
                    operation = "handle_print_error",
                    "live-only printer operation reached durable queued-command conversion"
                );
                return Err(Status::failed_precondition(
                    "print error operation requires live dispatch",
                ));
            }
            let required_device_features = payload
                .operation
                .required_device_features()
                .iter()
                .map(|feature| feature.proto_value())
                .collect();
            hub_command::Command::PrinterOperation(PrinterOperation {
                serial_number: payload.serial_number,
                required_device_features,
                operation: Some(proto_printer_operation(payload.operation)),
            })
        }
        "link_printer" => {
            tracing::error!(
                command_id = %command.id,
                "link printer command reached durable queued-command conversion"
            );
            return Err(Status::failed_precondition(
                "link printer command requires live secret dispatch",
            ));
        }
        "firmware_refresh" | "firmware_control" => {
            tracing::error!(
                command_id = %command.id,
                command_kind = %command.kind,
                "live-only firmware command reached durable queued-command conversion"
            );
            return Err(Status::failed_precondition(
                "firmware command requires live session dispatch",
            ));
        }
        "print_project_file" => {
            let mut payload: PrintProjectFilePayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize print project file command payload"
                    );
                    Status::internal("invalid print command payload")
                })?;
            if options.require_artifact_download_path
                && payload.artifact_download_path.trim().is_empty()
            {
                return Err(Status::internal("missing artifact download path"));
            }
            let (print_options, task_metadata, submission_source) =
                match payload.studio_metadata.take() {
                    Some(metadata) => {
                        let (options, task) = proto_studio_metadata(metadata);
                        (options, Some(task), PrintSubmissionSource::Studio as i32)
                    }
                    None => proto_web_options(&payload)
                        .map(|options| (options, None, PrintSubmissionSource::Web as i32))
                        .map_err(|err| {
                            tracing::error!(
                                command_id = %command.id,
                                error = %format!("{err:#}"),
                                "failed to deserialize web print mappings"
                            );
                            Status::internal("invalid print command payload")
                        })?,
                };
            hub_command::Command::PrintProjectFile(PrintProjectFile {
                job_id: payload.job_id,
                artifact_id: payload.artifact_id,
                printer_id: payload.printer_id,
                serial_number: payload.serial_number,
                filename: payload.filename,
                storage_path: payload.storage_path,
                artifact_download_path: payload.artifact_download_path,
                size_bytes: payload.size_bytes,
                plate_id: payload.plate_id,
                studio_submission_id: payload.studio_submission_id.get() as u32,
                options: Some(print_options),
                task_metadata,
                submission_source,
            })
        }
        kind => {
            tracing::error!(%command_id, %kind, "unknown persisted command kind");
            return Err(Status::internal("unknown persisted command kind"));
        }
    };

    Ok(HubCommand {
        command_id,
        command: Some(command),
    })
}

pub fn live_printer_operation_hub_command(
    command_id: CommandId,
    serial_number: String,
    operation: PrinterOperationKind,
) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::PrinterOperation(PrinterOperation {
            serial_number,
            required_device_features: Vec::new(),
            operation: Some(proto_printer_operation(operation)),
        })),
    }
}

fn proto_studio_metadata(
    metadata: StudioPrintMetadata,
) -> (PrintProjectFileOptions, StudioTaskMetadata) {
    let StudioPrintMetadata::V1(metadata) = metadata;
    let options = PrintProjectFileOptions {
        use_ams: metadata.task_use_ams,
        bed_leveling: metadata.task_bed_leveling,
        flow_cali: metadata.task_flow_cali,
        vibration_cali: metadata.task_vibration_cali,
        layer_inspect: metadata.task_layer_inspect,
        record_timelapse: metadata.task_record_timelapse,
        timelapse_use_internal: metadata.task_timelapse_use_internal,
        bed_type: metadata.task_bed_type,
        auto_bed_leveling: Some(i32::from(metadata.auto_bed_leveling.as_u8())),
        auto_flow_cali: Some(i32::from(metadata.auto_flow_cali.as_u8())),
        auto_offset_cali: Some(i32::from(metadata.auto_offset_cali.as_u8())),
        extruder_cali_manual_mode: Some(i32::from(metadata.extruder_cali_manual_mode)),
        try_emmc_print: metadata.try_emmc_print,
        nozzle_mapping: metadata.nozzle_mapping,
        ams_mapping: metadata.ams_mapping,
        ams_mapping2: metadata
            .ams_mapping2
            .into_iter()
            .map(|entry| StudioAmsMappingEntry {
                ams_id: entry.ams_id,
                slot_id: entry.slot_id,
            })
            .collect(),
        ams_mapping_info: metadata
            .ams_mapping_info
            .into_iter()
            .map(|entry| StudioAmsMappingInfo {
                ams: entry.ams,
                target_color: entry.target_color,
                filament_id: entry.filament_id,
                filament_type: entry.filament_type,
                nozzle_id: entry.nozzle_id,
                source_color: entry.source_color,
            })
            .collect(),
        nozzles_info: metadata
            .nozzles_info
            .into_iter()
            .map(|entry| StudioNozzleInfo {
                id: entry.id,
                nozzle_type: entry.nozzle_type,
                flow_size: entry.flow_size,
                diameter: entry.diameter.map(|value| value.get()),
            })
            .collect(),
    };
    let task = StudioTaskMetadata {
        task_name: metadata.task_name,
        project_name: metadata.project_name,
        preset_name: metadata.preset_name,
        connection_type: metadata.connection_type,
        comments: metadata.comments,
        origin_profile_id: metadata.origin_profile_id,
        stl_design_id: metadata.stl_design_id,
        origin_model_id: metadata.origin_model_id,
        print_type: metadata.print_type,
        submitted_device_name: metadata.submitted_device_name,
        svc_context: metadata.svc_context,
        slicer_uid: metadata.slicer_uid,
    };
    (options, task)
}

fn proto_web_options(
    payload: &PrintProjectFilePayload,
) -> Result<PrintProjectFileOptions, serde_json::Error> {
    let ams_mapping: Vec<i32> = parse_optional_json(&payload.ams_mapping_json)?;
    let ams_mapping2: Vec<pandar_core::StudioAmsMappingEntry> =
        parse_optional_json(&payload.ams_mapping2_json)?;
    let ams_mapping_info: Vec<pandar_core::StudioAmsMappingInfo> =
        parse_optional_json(&payload.ams_mapping_info_json)?;
    let options = PrintProjectFileOptions {
        use_ams: payload.use_ams,
        bed_leveling: payload.bed_leveling,
        flow_cali: payload.flow_cali,
        vibration_cali: false,
        layer_inspect: false,
        record_timelapse: payload.timelapse,
        timelapse_use_internal: false,
        bed_type: "auto".to_owned(),
        auto_bed_leveling: Some(i32::from(payload.auto_bed_leveling.as_u8())),
        auto_flow_cali: Some(i32::from(payload.auto_flow_cali.as_u8())),
        auto_offset_cali: Some(i32::from(payload.auto_offset_cali.as_u8())),
        extruder_cali_manual_mode: None,
        try_emmc_print: true,
        nozzle_mapping: Vec::new(),
        ams_mapping,
        ams_mapping2: ams_mapping2
            .into_iter()
            .map(|entry| StudioAmsMappingEntry {
                ams_id: entry.ams_id,
                slot_id: entry.slot_id,
            })
            .collect(),
        ams_mapping_info: ams_mapping_info
            .into_iter()
            .map(|entry| StudioAmsMappingInfo {
                ams: entry.ams,
                target_color: entry.target_color,
                filament_id: entry.filament_id,
                filament_type: entry.filament_type,
                nozzle_id: entry.nozzle_id,
                source_color: entry.source_color,
            })
            .collect(),
        nozzles_info: Vec::new(),
    };
    Ok(options)
}

fn parse_optional_json<T>(value: &Option<String>) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned + Default,
{
    value
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map(Option::unwrap_or_default)
}
