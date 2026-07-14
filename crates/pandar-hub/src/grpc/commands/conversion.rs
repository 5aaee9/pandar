use pandar_core::{CommandId, CommandRecord};
use tonic::Status;

mod operations;

use operations::proto_printer_operation;

use crate::{
    material_mapping::{AmsMapping2Entry, AmsMappingInfoEntry, validate_mapping_len},
    protocol::agent::v1::{
        DiagnosePrinter, DiscoverPrinters, HubCommand, PrintProjectFile, PrinterOperation,
        RefreshPrinterMaterials, RefreshPrinters, ReloadPrinterConnection, hub_command,
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
            let payload: PrintProjectFilePayload = serde_json::from_str(&command.payload_json)
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
                use_ams: payload.use_ams,
                flow_cali: payload.flow_cali,
                timelapse: payload.timelapse,
                ams_mapping_json: mapping_payload_string(
                    payload.ams_mapping_json.as_deref(),
                    "ams_mapping_json",
                    &command_id,
                )?,
                ams_mapping2_json: mapping_payload_string(
                    payload.ams_mapping2_json.as_deref(),
                    "ams_mapping2_json",
                    &command_id,
                )?,
                ams_mapping_info_json: mapping_payload_string(
                    payload.ams_mapping_info_json.as_deref(),
                    "ams_mapping_info_json",
                    &command_id,
                )?,
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

fn mapping_payload_string(
    value: Option<&str>,
    field: &'static str,
    command_id: &str,
) -> Result<String, Status> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    match field {
        "ams_mapping_json" => {
            parse_mapping::<i32>(value, field, command_id)?;
        }
        "ams_mapping2_json" => {
            parse_mapping::<AmsMapping2Entry>(value, field, command_id)?;
        }
        "ams_mapping_info_json" => {
            parse_mapping::<AmsMappingInfoEntry>(value, field, command_id)?;
        }
        _ => unreachable!("print mapping field should be known"),
    }
    Ok(value.to_string())
}

fn parse_mapping<T: serde::de::DeserializeOwned>(
    value: &str,
    field: &'static str,
    command_id: &str,
) -> Result<Vec<T>, Status> {
    let mapping = serde_json::from_str::<Vec<T>>(value).map_err(|err| {
        let err = anyhow::Error::from(err).context(format!(
            "failed to parse persisted {field} for print command"
        ));
        tracing::error!(
            %command_id,
            %field,
            error = %format!("{err:#}"),
            "failed to serialize print command mapping"
        );
        Status::internal("invalid print command mapping payload")
    })?;
    if !validate_mapping_len(mapping.len()) {
        tracing::error!(
            %command_id,
            %field,
            "print command mapping contains too many entries"
        );
        return Err(Status::internal("invalid print command mapping payload"));
    }
    Ok(mapping)
}
