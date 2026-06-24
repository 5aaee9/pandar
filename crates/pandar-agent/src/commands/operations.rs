use anyhow::Context;
use tokio::sync::mpsc;

use super::{
    BambuMachineGateway, ack_event, failure_event, rejected_ack_event, success_event_with_result,
};
use crate::{
    AgentConfig,
    machine::{PrinterAxis as MachinePrinterAxis, PrinterOperation as MachinePrinterOperation},
    protocol::agent::v1::{
        AgentEvent, Axis, PrinterOperation as ProtoPrinterOperation, printer_operation,
    },
};

const MAX_MOVE_DELTA_MM: f64 = 50.0;
const MIN_MOVE_FEEDRATE_MM_PER_MIN: u32 = 1;
const MAX_MOVE_FEEDRATE_MM_PER_MIN: u32 = 12_000;
const MAX_HOTEND_TEMPERATURE_CELSIUS: u32 = 300;

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
            sender
                .send(rejected_ack_event(config, command_id, format!("{err:#}")))
                .await
                .context("queue printer-operation rejected ack")?;
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
        Ok(()) => {
            let result_json = printer_operation_result_json(&command.serial_number, &operation);
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue printer-operation command success")?;
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

fn parse_printer_operation(
    command: &ProtoPrinterOperation,
) -> anyhow::Result<MachinePrinterOperation> {
    match command.operation.as_ref() {
        Some(printer_operation::Operation::Pause(_)) => Ok(MachinePrinterOperation::Pause),
        Some(printer_operation::Operation::Resume(_)) => Ok(MachinePrinterOperation::Resume),
        Some(printer_operation::Operation::Stop(_)) => Ok(MachinePrinterOperation::Stop),
        Some(printer_operation::Operation::SetPrintSpeed(operation)) => {
            match operation.speed_mode {
                1..=4 => Ok(MachinePrinterOperation::SetPrintSpeed(
                    operation.speed_mode as u8,
                )),
                _ => anyhow::bail!("invalid printer operation speed_mode; expected 1..=4"),
            }
        }
        Some(printer_operation::Operation::Home(operation)) => {
            let axes = operation
                .axes
                .iter()
                .copied()
                .map(parse_printer_axis)
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(MachinePrinterOperation::Home { axes })
        }
        Some(printer_operation::Operation::MoveAxes(operation)) => {
            let mut x_mm = None;
            let mut y_mm = None;
            let mut z_mm = None;
            for movement in &operation.movements {
                validate_move_delta(movement.delta_mm)?;
                match parse_printer_axis(movement.axis)? {
                    MachinePrinterAxis::X if x_mm.is_none() => x_mm = Some(movement.delta_mm),
                    MachinePrinterAxis::Y if y_mm.is_none() => y_mm = Some(movement.delta_mm),
                    MachinePrinterAxis::Z if z_mm.is_none() => z_mm = Some(movement.delta_mm),
                    _ => anyhow::bail!("printer operation move_axes contains duplicate axis"),
                }
            }
            if x_mm.is_none() && y_mm.is_none() && z_mm.is_none() {
                anyhow::bail!("printer operation move_axes requires at least one axis");
            }
            let feedrate_mm_per_min = parse_move_feedrate(operation.feedrate_mm_per_min)?;
            Ok(MachinePrinterOperation::MoveAxes {
                x_mm,
                y_mm,
                z_mm,
                feedrate_mm_per_min,
            })
        }
        Some(printer_operation::Operation::SetHotendTemperature(operation)) => {
            if operation.temperature_celsius > MAX_HOTEND_TEMPERATURE_CELSIUS {
                anyhow::bail!("invalid printer operation hotend temperature; expected <= 300");
            }
            Ok(MachinePrinterOperation::SetHotendTemperature {
                temperature_celsius: operation.temperature_celsius as u16,
                wait: operation.wait,
            })
        }
        None => anyhow::bail!("missing printer operation"),
    }
}

fn validate_move_delta(delta_mm: f64) -> anyhow::Result<()> {
    if delta_mm.is_finite() && delta_mm != 0.0 && delta_mm.abs() <= MAX_MOVE_DELTA_MM {
        Ok(())
    } else {
        anyhow::bail!(
            "invalid printer operation move_axes delta_mm; expected finite nonzero value within 50mm"
        )
    }
}

fn parse_move_feedrate(feedrate_mm_per_min: u32) -> anyhow::Result<Option<f64>> {
    if feedrate_mm_per_min == 0 {
        return Ok(None);
    }
    if (MIN_MOVE_FEEDRATE_MM_PER_MIN..=MAX_MOVE_FEEDRATE_MM_PER_MIN).contains(&feedrate_mm_per_min)
    {
        Ok(Some(feedrate_mm_per_min as f64))
    } else {
        anyhow::bail!("invalid printer operation move_axes feedrate; expected 1..=12000")
    }
}

fn parse_printer_axis(axis: i32) -> anyhow::Result<MachinePrinterAxis> {
    match Axis::try_from(axis) {
        Ok(Axis::X) => Ok(MachinePrinterAxis::X),
        Ok(Axis::Y) => Ok(MachinePrinterAxis::Y),
        Ok(Axis::Z) => Ok(MachinePrinterAxis::Z),
        Ok(Axis::Unspecified) | Err(_) => anyhow::bail!("invalid printer operation axis"),
    }
}

fn printer_operation_action(operation: &MachinePrinterOperation) -> &'static str {
    match operation {
        MachinePrinterOperation::Pause => "pause",
        MachinePrinterOperation::Resume => "resume",
        MachinePrinterOperation::Stop => "stop",
        MachinePrinterOperation::SetPrintSpeed(_) => "set_print_speed",
        MachinePrinterOperation::Home { .. } => "home",
        MachinePrinterOperation::MoveAxes { .. } => "move_axes",
        MachinePrinterOperation::SetHotendTemperature { .. } => "set_hotend_temperature",
    }
}

fn printer_operation_result_json(
    serial_number: &str,
    operation: &MachinePrinterOperation,
) -> String {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), serde_json::json!("printer_operation"));
    result.insert(
        "action".to_string(),
        serde_json::json!(printer_operation_action(operation)),
    );
    result.insert(
        "serial_number".to_string(),
        serde_json::json!(serial_number),
    );
    match operation {
        MachinePrinterOperation::SetPrintSpeed(speed_mode) => {
            result.insert("speed_mode".to_string(), serde_json::json!(speed_mode));
        }
        MachinePrinterOperation::Home { axes } => {
            result.insert(
                "axes".to_string(),
                serde_json::json!(
                    axes.iter()
                        .map(|axis| match axis {
                            MachinePrinterAxis::X => "x",
                            MachinePrinterAxis::Y => "y",
                            MachinePrinterAxis::Z => "z",
                        })
                        .collect::<Vec<_>>()
                ),
            );
        }
        MachinePrinterOperation::MoveAxes {
            x_mm,
            y_mm,
            z_mm,
            feedrate_mm_per_min,
        } => {
            if let Some(value) = x_mm {
                result.insert("x_mm".to_string(), serde_json::json!(value));
            }
            if let Some(value) = y_mm {
                result.insert("y_mm".to_string(), serde_json::json!(value));
            }
            if let Some(value) = z_mm {
                result.insert("z_mm".to_string(), serde_json::json!(value));
            }
            if let Some(value) = feedrate_mm_per_min {
                result.insert("feedrate_mm_per_min".to_string(), serde_json::json!(value));
            }
        }
        MachinePrinterOperation::SetHotendTemperature {
            temperature_celsius,
            wait,
        } => {
            result.insert(
                "temperature_celsius".to_string(),
                serde_json::json!(temperature_celsius),
            );
            result.insert("wait".to_string(), serde_json::json!(wait));
        }
        MachinePrinterOperation::Pause
        | MachinePrinterOperation::Resume
        | MachinePrinterOperation::Stop => {}
    }
    serde_json::Value::Object(result).to_string()
}
