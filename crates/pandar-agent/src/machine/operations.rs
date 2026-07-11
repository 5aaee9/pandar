use anyhow::Context;

mod light;
mod report;

use super::{
    BambuPrinterEndpoint, PrinterOperationDispatchResult,
    mqtt::{
        AmsFilamentCommand, AmsSlotCommand, BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics,
        BambuMqttTransport, GcodeLineCommand, HandlePrintErrorCommand, PrintErrorAction,
        PrintSpeed, PublishedMqttCommand, SetNozzleTemperatureCommand,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    HandlePrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    ToggleLight,
    SetChamberLight(bool),
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
    let commands = match operation {
        PrinterOperation::ToggleLight => light::chamber_light_commands(mqtt, &topics, None).await?,
        PrinterOperation::SetChamberLight(on) => {
            light::chamber_light_commands(mqtt, &topics, Some(on)).await?
        }
        operation => vec![mqtt_command_for_printer_operation(operation)?],
    };
    let command_payloads = commands
        .iter()
        .map(BambuMqttCommand::command_payload)
        .collect::<Vec<_>>();
    let sequence_ids = command_payloads
        .iter()
        .filter_map(|payload| payload.sequence_id.clone())
        .collect::<Vec<_>>();
    for command_payload in command_payloads {
        mqtt.publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: command_payload.payload,
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish printer operation to {}", endpoint.serial))?;
    }

    if sequence_ids.is_empty() {
        return Ok(PrinterOperationDispatchResult::dispatched());
    }

    match matching_sequence_report(mqtt, &sequence_ids).await {
        Ok((sequence_id, report)) => {
            let error = report.error();
            let mqtt_summary = report.summary();
            Ok(PrinterOperationDispatchResult {
                sequence_id: Some(sequence_id),
                error,
                mqtt_report: Some(report.into_payload()),
                mqtt_summary,
            })
        }
        Err(err) => {
            let sequence_id = sequence_ids
                .last()
                .expect("sequence ids are not empty")
                .clone();
            tracing::warn!(
                serial = %endpoint.serial,
                sequence_id = %sequence_id,
                error = %format!("{err:#}"),
                "printer operation result report unavailable"
            );
            Ok(PrinterOperationDispatchResult {
                sequence_id: Some(sequence_id),
                mqtt_report: None,
                mqtt_summary: None,
                error: None,
            })
        }
    }
}

pub(super) fn mqtt_command_for_printer_operation(
    operation: PrinterOperation,
) -> anyhow::Result<BambuMqttCommand> {
    match operation {
        PrinterOperation::Pause => Ok(BambuMqttCommand::PausePrint),
        PrinterOperation::Resume => Ok(BambuMqttCommand::ResumePrint),
        PrinterOperation::Stop => Ok(BambuMqttCommand::StopPrint),
        PrinterOperation::HandlePrintError {
            error_action,
            print_error,
            printer_job_id,
            sequence_id,
        } => Ok(BambuMqttCommand::HandlePrintError(
            HandlePrintErrorCommand {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            },
        )),
        PrinterOperation::ToggleLight => {
            unreachable!("toggle_light is handled before payload mapping")
        }
        PrinterOperation::SetChamberLight(_) => {
            unreachable!("set_chamber_light is handled before payload mapping")
        }
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

async fn matching_sequence_report<T>(
    mqtt: &T,
    sequence_ids: &[String],
) -> anyhow::Result<(String, report::OperationReport)>
where
    T: BambuMqttTransport + Send + Sync,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut failures = Vec::new();
        loop {
            let report = mqtt
                .next_report(std::time::Duration::from_secs(5))
                .await
                .context("wait for printer operation MQTT result")?;
            let Some(report) = report::OperationReport::from_payload(&report) else {
                continue;
            };
            let Some(sequence_id) = report.sequence_id() else {
                continue;
            };
            if !sequence_ids.contains(&sequence_id) {
                continue;
            }
            if report.error().is_none() {
                return Ok((sequence_id, report));
            }
            failures.push((sequence_id, report));
            if failures.len() == sequence_ids.len() {
                return Ok(failures.remove(0));
            }
        }
    })
    .await
    .context("wait for matching printer operation MQTT result")?
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
