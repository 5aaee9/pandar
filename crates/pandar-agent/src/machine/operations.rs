use anyhow::Context;
use serde_json::Value;

use super::{
    BambuPrinterEndpoint, PrinterOperationDispatchResult,
    mqtt::{
        AmsFilamentCommand, AmsSlotCommand, BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics,
        BambuMqttTransport, GcodeLineCommand, PrintSpeed, PublishedMqttCommand,
        SetNozzleTemperatureCommand,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed(u8),
    SelectExtruder(u32),
    Home {
        axes: Vec<PrinterAxis>,
    },
    MoveAxes {
        x_mm: Option<f64>,
        y_mm: Option<f64>,
        z_mm: Option<f64>,
        feedrate_mm_per_min: Option<f64>,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u32>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    AmsRereadRfid {
        ams_id: u32,
        slot_id: u32,
    },
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsUnloadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

pub(super) async fn dispatch_printer_operation<T>(
    endpoint: &BambuPrinterEndpoint,
    mqtt: &T,
    operation: PrinterOperation,
) -> anyhow::Result<PrinterOperationDispatchResult>
where
    T: BambuMqttTransport + Send + Sync,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    mqtt.subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    let payload = mqtt_command_for_printer_operation(operation)?.payload();
    let sequence_id = command_sequence_id(&payload);
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request.clone(),
        payload,
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .with_context(|| format!("publish printer operation to {}", endpoint.serial))?;

    let Some(sequence_id) = sequence_id else {
        return Ok(PrinterOperationDispatchResult::dispatched());
    };

    match matching_sequence_report(mqtt, &sequence_id).await {
        Ok(report) => Ok(PrinterOperationDispatchResult {
            sequence_id: Some(sequence_id),
            error: printer_operation_report_error(&report),
            mqtt_report: Some(report),
        }),
        Err(err) => {
            tracing::warn!(
                serial = %endpoint.serial,
                sequence_id = %sequence_id,
                error = %format!("{err:#}"),
                "printer operation result report unavailable"
            );
            Ok(PrinterOperationDispatchResult {
                sequence_id: Some(sequence_id),
                mqtt_report: None,
                error: None,
            })
        }
    }
}

fn mqtt_command_for_printer_operation(
    operation: PrinterOperation,
) -> anyhow::Result<BambuMqttCommand> {
    match operation {
        PrinterOperation::Pause => Ok(BambuMqttCommand::PausePrint),
        PrinterOperation::Resume => Ok(BambuMqttCommand::ResumePrint),
        PrinterOperation::Stop => Ok(BambuMqttCommand::StopPrint),
        PrinterOperation::SetPrintSpeed(mode) => {
            Ok(BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(mode)?))
        }
        PrinterOperation::SelectExtruder(extruder_id) => {
            Ok(BambuMqttCommand::SelectExtruder(extruder_id))
        }
        PrinterOperation::Home { .. } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec!["G28".to_string()],
        })),
        PrinterOperation::MoveAxes {
            x_mm,
            y_mm,
            z_mm,
            feedrate_mm_per_min,
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec![
                "G91".to_string(),
                move_axes_gcode_line(x_mm, y_mm, z_mm, feedrate_mm_per_min),
                "G90".to_string(),
            ],
        })),
        PrinterOperation::SetHotendTemperature {
            temperature_celsius,
            wait,
            extruder_id,
        } => match extruder_id {
            Some(extruder_id) => Ok(BambuMqttCommand::SetNozzleTemperature(
                SetNozzleTemperatureCommand {
                    extruder_id,
                    target_temp: temperature_celsius,
                },
            )),
            None => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
                lines: vec![format!(
                    "{} S{}",
                    if wait { "M109" } else { "M104" },
                    temperature_celsius
                )],
            })),
        },
        PrinterOperation::SetBedTemperature {
            temperature_celsius,
            wait,
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec![format!(
                "{} S{}",
                if wait { "M190" } else { "M140" },
                temperature_celsius
            )],
        })),
        PrinterOperation::SetChamberTemperature {
            temperature_celsius,
            wait,
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: if wait {
                vec![
                    "M106 P2 S255".to_string(),
                    format!("M191 S{}", temperature_celsius),
                    "M106 P2 S0".to_string(),
                ]
            } else {
                vec![format!("M141 S{}", temperature_celsius)]
            },
        })),
        PrinterOperation::AmsRereadRfid { ams_id, slot_id } => {
            Ok(BambuMqttCommand::AmsRereadRfid(AmsSlotCommand {
                ams_id,
                slot_id,
            }))
        }
        PrinterOperation::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => Ok(BambuMqttCommand::AmsLoadFilament(AmsFilamentCommand {
            ams_id: ams_command_ams_id(ams_id, external_id.as_deref()),
            slot_id: ams_command_slot_id(slot_id, external_id.as_deref()),
            target: global_tray_id.unwrap_or(slot_id),
            extruder_id,
        })),
        PrinterOperation::AmsUnloadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => Ok(BambuMqttCommand::AmsUnloadFilament(AmsFilamentCommand {
            ams_id: ams_command_ams_id(ams_id, external_id.as_deref()),
            slot_id: ams_command_slot_id(slot_id, external_id.as_deref()),
            target: global_tray_id.unwrap_or(slot_id),
            extruder_id,
        })),
    }
}

fn ams_command_ams_id(ams_id: u32, external_id: Option<&str>) -> u32 {
    if external_id.is_some() { 255 } else { ams_id }
}

fn ams_command_slot_id(slot_id: u32, external_id: Option<&str>) -> u32 {
    if matches!(external_id, Some("254")) {
        254
    } else {
        slot_id
    }
}

async fn matching_sequence_report<T>(mqtt: &T, sequence_id: &str) -> anyhow::Result<Value>
where
    T: BambuMqttTransport + Send + Sync,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let report = mqtt
                .next_report(std::time::Duration::from_secs(5))
                .await
                .context("wait for printer operation MQTT result")?;
            if report_sequence_id(&report).as_deref() == Some(sequence_id) {
                return Ok(report);
            }
        }
    })
    .await
    .context("wait for matching printer operation MQTT result")?
}

fn command_sequence_id(payload: &Value) -> Option<String> {
    ["print", "info", "pushing", "system", "camera"]
        .into_iter()
        .find_map(|section| payload.get(section).and_then(section_sequence_id))
}

fn report_sequence_id(report: &Value) -> Option<String> {
    ["print", "info", "pushing", "system", "camera"]
        .into_iter()
        .find_map(|section| report.get(section).and_then(section_sequence_id))
}

fn section_sequence_id(value: &Value) -> Option<String> {
    match value.get("sequence_id")? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn printer_operation_report_error(report: &Value) -> Option<String> {
    let print = report.get("print")?;
    if print.get("result").and_then(Value::as_str) == Some("fail") {
        return Some(
            report_error_message(print).unwrap_or_else(|| "printer reported failure".to_owned()),
        );
    }
    for key in ["err_code", "errno"] {
        if let Some(code) = print.get(key).and_then(Value::as_i64)
            && code != 0
        {
            return Some(
                report_error_message(print)
                    .unwrap_or_else(|| format!("printer reported {key} {code}")),
            );
        }
    }
    None
}

fn report_error_message(value: &Value) -> Option<String> {
    ["reason", "message", "msg", "error"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn move_axes_gcode_line(
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
) -> String {
    let mut line = String::from("G0");
    if let Some(value) = x_mm {
        line.push_str(&format!(" X{}", format_gcode_number(value)));
    }
    if let Some(value) = y_mm {
        line.push_str(&format!(" Y{}", format_gcode_number(value)));
    }
    if let Some(value) = z_mm {
        line.push_str(&format!(" Z{}", format_gcode_number(value)));
    }
    if let Some(value) = feedrate_mm_per_min {
        line.push_str(&format!(" F{}", format_gcode_number(value)));
    }
    line
}

fn format_gcode_number(value: f64) -> String {
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
