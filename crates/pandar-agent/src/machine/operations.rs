use anyhow::Context;

use super::{
    BambuPrinterEndpoint,
    mqtt::{
        BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, GcodeLineCommand,
        PrintSpeed, PublishedMqttCommand,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    SetPrintSpeed(u8),
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
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + Send + Sync,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request,
        payload: mqtt_command_for_printer_operation(operation)?.payload(),
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .with_context(|| format!("publish printer operation to {}", endpoint.serial))
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
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec![format!(
                "{} S{}",
                if wait { "M109" } else { "M104" },
                temperature_celsius
            )],
        })),
    }
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
