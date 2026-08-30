use anyhow::Context;
use pandar_core::{
    PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperation, RequiredDeviceFeature,
};
use tokio::sync::mpsc;

mod ams;
mod h2c;

use super::{
    BambuMachineGateway, ack_event, failure_event, failure_event_with_result,
    printer_materials_snapshot_event, reject_protocol_command, rejected_ack_event,
    success_event_with_result,
};
use crate::{
    AgentConfig,
    commands::operation_results::{printer_operation_action, printer_operation_result_json},
};
use pandar_protocol::agent::v1::{
    AgentEvent, Axis, DeviceFeature, PrintErrorAction as ProtoPrintErrorAction,
    PrinterOperation as ProtoPrinterOperation, printer_operation,
};

pub(super) async fn emit_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: ProtoPrinterOperation,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let operation = match parse_printer_operation(&command) {
        Ok(operation) => operation,
        Err(err) => {
            reject_protocol_command(config, sender, command_id, format!("{err:#}")).await?;
            return Ok(());
        }
    };

    if let Err(err) = gateway.validate_printer(&command.serial_number).await {
        let error = gateway.redact_error(&format!("{err:#}"));
        sender
            .send(rejected_ack_event(config, command_id, error))
            .await
            .context("queue printer-operation rejected ack")?;
        return Ok(());
    }

    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue printer-operation command ack")?;

    match gateway
        .operate_printer(&command.serial_number, operation.clone())
        .await
        .with_context(|| {
            format!(
                "dispatch printer operation {} to {}",
                printer_operation_action(&operation),
                command.serial_number
            )
        }) {
        Ok(dispatch_result) => {
            if ams::refresh_materials_after_operation(&operation) {
                match gateway
                    .refresh_printer_materials(&command.serial_number, None)
                    .await
                    .with_context(|| {
                        format!(
                            "refresh printer materials after {} for {}",
                            printer_operation_action(&operation),
                            command.serial_number
                        )
                    }) {
                    Ok(materials) => {
                        sender
                            .send(printer_materials_snapshot_event(config, materials))
                            .await
                            .context("queue printer materials snapshot event")?;
                    }
                    Err(err) => {
                        let error = gateway.redact_error(&format!("{err:#}"));
                        sender
                            .send(failure_event(config, command_id, error))
                            .await
                            .context("queue printer-operation command failure")?;
                        return Ok(());
                    }
                }
            }
            let result_json =
                printer_operation_result_json(&command.serial_number, &operation, &dispatch_result);
            if let Some(error) = dispatch_result.error {
                sender
                    .send(failure_event_with_result(
                        config,
                        command_id,
                        error,
                        result_json,
                    ))
                    .await
                    .context("queue printer-operation command failure")?;
            } else {
                sender
                    .send(success_event_with_result(config, command_id, result_json))
                    .await
                    .context("queue printer-operation command success")?;
            }
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue printer-operation command failure")?;
        }
    }

    Ok(())
}

fn parse_printer_operation(command: &ProtoPrinterOperation) -> anyhow::Result<PrinterOperation> {
    let required_device_features =
        parse_required_device_features(&command.required_device_features)?;
    if !required_device_features.is_empty()
        && !matches!(
            command.operation.as_ref(),
            Some(printer_operation::Operation::Home(_))
                | Some(printer_operation::Operation::MoveAxes(_))
        )
    {
        anyhow::bail!("required device feature is only valid for home or move_axes");
    }
    let operation = match command.operation.as_ref() {
        Some(printer_operation::Operation::Pause(_)) => PrinterOperation::Pause {},
        Some(printer_operation::Operation::Resume(_)) => PrinterOperation::Resume {},
        Some(printer_operation::Operation::Stop(_)) => PrinterOperation::Stop {},
        Some(printer_operation::Operation::ToggleLight(_)) => PrinterOperation::ToggleLight {},
        Some(printer_operation::Operation::SetChamberLight(operation)) => {
            PrinterOperation::SetChamberLight { on: operation.on }
        }
        Some(printer_operation::Operation::SetPrintSpeed(operation)) => {
            PrinterOperation::SetPrintSpeed {
                speed_mode: u8::try_from(operation.speed_mode)
                    .context("printer operation speed_mode exceeds uint8")?,
            }
        }
        Some(printer_operation::Operation::SetFanSpeed(operation)) => {
            PrinterOperation::SetFanSpeed {
                fan_index: u8::try_from(operation.fan_index)
                    .context("printer operation fan_index exceeds uint8")?,
                speed_percent: u8::try_from(operation.speed_percent)
                    .context("printer operation fan speed exceeds uint8")?,
                airduct: operation.airduct,
            }
        }
        Some(printer_operation::Operation::SelectExtruder(operation)) => {
            PrinterOperation::SelectExtruder {
                extruder_id: operation.extruder_id,
            }
        }
        Some(printer_operation::Operation::Home(operation)) => PrinterOperation::Home {
            axes: operation
                .axes
                .iter()
                .copied()
                .map(parse_printer_axis)
                .collect::<anyhow::Result<Vec<_>>>()?,
            required_device_features,
        },
        Some(printer_operation::Operation::MoveAxes(operation)) => PrinterOperation::MoveAxes {
            movements: operation
                .movements
                .iter()
                .map(|movement| {
                    Ok(PrinterAxisMovement {
                        axis: parse_printer_axis(movement.axis)?,
                        delta_mm: movement.delta_mm,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
            feedrate_mm_per_min: (operation.feedrate_mm_per_min != 0)
                .then_some(operation.feedrate_mm_per_min),
            required_device_features,
        },
        Some(printer_operation::Operation::SetHotendTemperature(operation)) => {
            PrinterOperation::SetHotendTemperature {
                temperature_celsius: u16::try_from(operation.temperature_celsius)
                    .context("printer operation hotend temperature exceeds uint16")?,
                wait: operation.wait,
                extruder_id: operation.extruder_id,
            }
        }
        Some(printer_operation::Operation::SetBedTemperature(operation)) => {
            PrinterOperation::SetBedTemperature {
                temperature_celsius: u16::try_from(operation.temperature_celsius)
                    .context("printer operation bed temperature exceeds uint16")?,
                wait: operation.wait,
            }
        }
        Some(printer_operation::Operation::SetChamberTemperature(operation)) => {
            PrinterOperation::SetChamberTemperature {
                temperature_celsius: u16::try_from(operation.temperature_celsius)
                    .context("printer operation chamber temperature exceeds uint16")?,
                wait: operation.wait,
            }
        }
        Some(
            operation @ (printer_operation::Operation::AmsRereadRfid(_)
            | printer_operation::Operation::AmsLoadFilament(_)
            | printer_operation::Operation::AmsUnloadFilament(_)
            | printer_operation::Operation::AmsStartDrying(_)
            | printer_operation::Operation::AmsStopDrying(_)),
        ) => ams::parse_ams_operation(operation)?,
        Some(printer_operation::Operation::HandlePrintError(operation)) => {
            let error_action = match ProtoPrintErrorAction::try_from(operation.error_action) {
                Ok(ProtoPrintErrorAction::Resume) => PrintErrorAction::Resume,
                Ok(ProtoPrintErrorAction::Ignore) => PrintErrorAction::Ignore,
                Ok(ProtoPrintErrorAction::Stop) => PrintErrorAction::Stop,
                Ok(ProtoPrintErrorAction::Unspecified) | Err(_) => {
                    anyhow::bail!("invalid printer operation error_action")
                }
            };
            PrinterOperation::HandlePrintError {
                error_action,
                print_error: operation.print_error,
                printer_job_id: operation.printer_job_id.clone(),
                sequence_id: operation.sequence_id,
            }
        }
        Some(printer_operation::Operation::GcodeLine(operation)) => PrinterOperation::GcodeLine {
            param: operation.param.clone(),
        },
        Some(printer_operation::Operation::GetAutoNozzleMapping(operation)) => {
            h2c::parse_auto_nozzle_mapping(operation)?
        }
        Some(printer_operation::Operation::NozzleHolderCtrl(operation)) => {
            PrinterOperation::NozzleHolderCtrl {
                action: operation.action,
            }
        }
        Some(printer_operation::Operation::NozzleInfoConfirm(operation)) => {
            PrinterOperation::NozzleInfoConfirm { id: operation.id }
        }
        Some(printer_operation::Operation::HolderNozzleRefresh(operation)) => {
            PrinterOperation::HolderNozzleRefresh { id: operation.id }
        }
        None => anyhow::bail!("missing printer operation"),
    };
    operation.validate().map_err(anyhow::Error::new)?;
    Ok(operation)
}

fn parse_required_device_features(values: &[i32]) -> anyhow::Result<Vec<RequiredDeviceFeature>> {
    let value = match values {
        [] => return Ok(Vec::new()),
        [value] => *value,
        _ => anyhow::bail!("printer operation contains duplicate required device feature values"),
    };
    match DeviceFeature::try_from(value) {
        Ok(DeviceFeature::BambuMqttHoming) => Ok(vec![RequiredDeviceFeature::BambuMqttHoming]),
        Ok(DeviceFeature::BambuMqttAxisControl) => {
            Ok(vec![RequiredDeviceFeature::BambuMqttAxisControl])
        }
        Ok(DeviceFeature::Unspecified) | Err(_) => {
            anyhow::bail!("invalid required device feature value {value}")
        }
    }
}

fn parse_printer_axis(axis: i32) -> anyhow::Result<PrinterAxis> {
    match Axis::try_from(axis) {
        Ok(Axis::X) => Ok(PrinterAxis::X),
        Ok(Axis::Y) => Ok(PrinterAxis::Y),
        Ok(Axis::Z) => Ok(PrinterAxis::Z),
        Ok(Axis::Unspecified) | Err(_) => anyhow::bail!("invalid printer operation axis"),
    }
}
