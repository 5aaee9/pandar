use pandar_core::CommandRecord;
use tonic::Status;

use crate::{
    protocol::agent::v1::{
        AmsLoadFilamentOperation, AmsRereadRfidOperation, AmsUnloadFilamentOperation, Axis,
        AxisMovement, DiagnosePrinter, DiscoverPrinters, HomeOperation, HubCommand,
        MoveAxesOperation, PauseOperation, PrintProjectFile, PrinterOperation,
        RefreshPrinterMaterials, RefreshPrinters, ResumeOperation, SelectExtruderOperation,
        SetBedTemperatureOperation, SetChamberLightOperation, SetChamberTemperatureOperation,
        SetHotendTemperatureOperation, SetPrintSpeedOperation, StopOperation, ToggleLightOperation,
        hub_command, printer_operation,
    },
    repositories::{
        DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload, PrinterAxis,
        PrinterAxisMovement, PrinterOperationKind, PrinterOperationPayload,
        RefreshPrinterMaterialsPayload,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandConversionOptions {
    pub require_artifact_download_path: bool,
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
        "printer_operation" => {
            let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize printer operation command payload"
                    );
                    Status::internal("invalid printer operation command payload")
                })?;
            hub_command::Command::PrinterOperation(PrinterOperation {
                serial_number: payload.serial_number,
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

fn proto_printer_operation(operation: PrinterOperationKind) -> printer_operation::Operation {
    match operation {
        PrinterOperationKind::Pause => printer_operation::Operation::Pause(PauseOperation {}),
        PrinterOperationKind::Resume => printer_operation::Operation::Resume(ResumeOperation {}),
        PrinterOperationKind::Stop => printer_operation::Operation::Stop(StopOperation {}),
        PrinterOperationKind::ToggleLight => {
            printer_operation::Operation::ToggleLight(ToggleLightOperation {})
        }
        PrinterOperationKind::SetChamberLight { on } => {
            printer_operation::Operation::SetChamberLight(SetChamberLightOperation { on })
        }
        PrinterOperationKind::SetPrintSpeed { speed_mode } => {
            printer_operation::Operation::SetPrintSpeed(SetPrintSpeedOperation {
                speed_mode: speed_mode.into(),
            })
        }
        PrinterOperationKind::SelectExtruder { extruder_id } => {
            printer_operation::Operation::SelectExtruder(SelectExtruderOperation { extruder_id })
        }
        PrinterOperationKind::Home { axes } => printer_operation::Operation::Home(HomeOperation {
            axes: axes.into_iter().map(proto_axis).collect(),
        }),
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
        } => printer_operation::Operation::MoveAxes(MoveAxesOperation {
            movements: movements.into_iter().map(proto_axis_movement).collect(),
            feedrate_mm_per_min: feedrate_mm_per_min.unwrap_or_default(),
        }),
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius,
            wait,
            extruder_id,
        } => printer_operation::Operation::SetHotendTemperature(SetHotendTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
            extruder_id,
        }),
        PrinterOperationKind::SetBedTemperature {
            temperature_celsius,
            wait,
        } => printer_operation::Operation::SetBedTemperature(SetBedTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
        }),
        PrinterOperationKind::SetChamberTemperature {
            temperature_celsius,
            wait,
        } => printer_operation::Operation::SetChamberTemperature(SetChamberTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
        }),
        PrinterOperationKind::AmsRereadRfid { ams_id, slot_id } => {
            printer_operation::Operation::AmsRereadRfid(AmsRereadRfidOperation { ams_id, slot_id })
        }
        PrinterOperationKind::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => printer_operation::Operation::AmsLoadFilament(AmsLoadFilamentOperation {
            ams_id,
            slot_id,
            global_tray_id: global_tray_id.unwrap_or_else(|| ams_id * 4 + slot_id),
            external_id: external_id.unwrap_or_default(),
            extruder_id,
        }),
        PrinterOperationKind::AmsUnloadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => printer_operation::Operation::AmsUnloadFilament(AmsUnloadFilamentOperation {
            ams_id,
            slot_id,
            global_tray_id: global_tray_id.unwrap_or_else(|| ams_id * 4 + slot_id),
            external_id: external_id.unwrap_or_default(),
            extruder_id,
        }),
    }
}

fn proto_axis(axis: PrinterAxis) -> i32 {
    match axis {
        PrinterAxis::X => Axis::X as i32,
        PrinterAxis::Y => Axis::Y as i32,
        PrinterAxis::Z => Axis::Z as i32,
    }
}

fn proto_axis_movement(movement: PrinterAxisMovement) -> AxisMovement {
    AxisMovement {
        axis: proto_axis(movement.axis),
        delta_mm: movement.delta_mm,
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
            parse_mapping::<Vec<i32>>(value, field, command_id)?;
        }
        "ams_mapping2_json" => {
            let entries = parse_mapping::<Vec<Mapping2Payload>>(value, field, command_id)?;
            for entry in entries {
                let _ = (entry.ams_id, entry.slot_id);
            }
        }
        _ => unreachable!("print mapping field should be known"),
    }
    Ok(value.to_string())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping2Payload {
    ams_id: i32,
    slot_id: i32,
}

fn parse_mapping<T: serde::de::DeserializeOwned>(
    value: &str,
    field: &'static str,
    command_id: &str,
) -> Result<T, Status> {
    serde_json::from_str::<T>(value).map_err(|err| {
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
    })
}
