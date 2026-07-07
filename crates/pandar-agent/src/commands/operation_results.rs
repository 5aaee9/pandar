use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use crate::machine::{
    PrinterAxis as MachinePrinterAxis, PrinterOperation as MachinePrinterOperation,
    PrinterOperationDispatchResult,
};

pub(super) fn printer_operation_action(operation: &MachinePrinterOperation) -> &'static str {
    match operation {
        MachinePrinterOperation::Pause => "pause",
        MachinePrinterOperation::Resume => "resume",
        MachinePrinterOperation::Stop => "stop",
        MachinePrinterOperation::ToggleLight => "toggle_light",
        MachinePrinterOperation::SetChamberLight(_) => "set_chamber_light",
        MachinePrinterOperation::SetPrintSpeed(_) => "set_print_speed",
        MachinePrinterOperation::SelectExtruder(_) => "select_extruder",
        MachinePrinterOperation::Home { .. } => "home",
        MachinePrinterOperation::MoveAxes { .. } => "move_axes",
        MachinePrinterOperation::SetHotendTemperature { .. } => "set_hotend_temperature",
        MachinePrinterOperation::SetBedTemperature { .. } => "set_bed_temperature",
        MachinePrinterOperation::SetChamberTemperature { .. } => "set_chamber_temperature",
        MachinePrinterOperation::AmsRereadRfid { .. } => "ams_reread_rfid",
        MachinePrinterOperation::AmsLoadFilament { .. } => "ams_load_filament",
        MachinePrinterOperation::AmsUnloadFilament { .. } => "ams_unload_filament",
    }
}

pub(super) fn printer_operation_result_json(
    serial_number: &str,
    operation: &MachinePrinterOperation,
    dispatch_result: &PrinterOperationDispatchResult,
) -> String {
    let mqtt = dispatch_result
        .mqtt_report
        .as_ref()
        .and_then(MqttReportSummary::from_report);
    let mqtt_report = dispatch_result
        .mqtt_report
        .as_ref()
        .and_then(MqttReportField::from_value);
    serde_json::to_string(&PrinterOperationResult {
        kind: "printer_operation",
        action: printer_operation_action(operation),
        serial_number,
        sequence_id: dispatch_result.sequence_id.as_deref(),
        mqtt_error: dispatch_result.error.as_deref(),
        mqtt_result: mqtt.as_ref().and_then(|summary| summary.result.as_ref()),
        mqtt_reason: mqtt.as_ref().and_then(|summary| summary.reason.as_ref()),
        mqtt_err_code: mqtt.as_ref().and_then(|summary| summary.err_code.as_ref()),
        mqtt_errno: mqtt.as_ref().and_then(|summary| summary.errno.as_ref()),
        mqtt_report: mqtt_report.as_ref(),
        operation: OperationResultFields::from(operation),
    })
    .expect("printer operation result is serializable")
}

#[derive(Serialize)]
struct PrinterOperationResult<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    action: &'static str,
    serial_number: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_result: Option<&'a MqttReportField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_reason: Option<&'a MqttReportField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_err_code: Option<&'a MqttReportField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_errno: Option<&'a MqttReportField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mqtt_report: Option<&'a MqttReportField>,
    #[serde(flatten)]
    operation: OperationResultFields,
}

#[derive(Default, Serialize)]
struct OperationResultFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_mode: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    axes: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    x_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    y_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    z_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedrate_mm_per_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature_celsius: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    light_on: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ams_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    global_tray_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_id: Option<String>,
}

#[derive(Deserialize)]
struct MqttReport {
    print: Option<MqttReportSection>,
    system: Option<MqttReportSection>,
}

#[derive(Deserialize)]
struct MqttReportSection {
    result: Option<MqttReportField>,
    reason: Option<MqttReportField>,
    err_code: Option<MqttReportField>,
    errno: Option<MqttReportField>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum MqttReportField {
    Object(BTreeMap<String, MqttReportField>),
    Array(Vec<MqttReportField>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl MqttReportField {
    fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

struct MqttReportSummary {
    result: Option<MqttReportField>,
    reason: Option<MqttReportField>,
    err_code: Option<MqttReportField>,
    errno: Option<MqttReportField>,
}

impl OperationResultFields {
    fn from(operation: &MachinePrinterOperation) -> Self {
        match operation {
            MachinePrinterOperation::SetPrintSpeed(speed_mode) => Self {
                speed_mode: Some(*speed_mode),
                ..Self::default()
            },
            MachinePrinterOperation::SelectExtruder(extruder_id) => Self {
                extruder_id: Some(*extruder_id),
                ..Self::default()
            },
            MachinePrinterOperation::Home { axes } => Self {
                axes: Some(axes.iter().map(axis_name).collect()),
                ..Self::default()
            },
            MachinePrinterOperation::MoveAxes {
                x_mm,
                y_mm,
                z_mm,
                feedrate_mm_per_min,
            } => Self {
                x_mm: *x_mm,
                y_mm: *y_mm,
                z_mm: *z_mm,
                feedrate_mm_per_min: *feedrate_mm_per_min,
                ..Self::default()
            },
            MachinePrinterOperation::SetHotendTemperature {
                temperature_celsius,
                wait,
                extruder_id,
            } => Self {
                temperature_celsius: Some(*temperature_celsius),
                wait: Some(*wait),
                extruder_id: *extruder_id,
                ..Self::default()
            },
            MachinePrinterOperation::SetBedTemperature {
                temperature_celsius,
                wait,
            }
            | MachinePrinterOperation::SetChamberTemperature {
                temperature_celsius,
                wait,
            } => Self {
                temperature_celsius: Some(*temperature_celsius),
                wait: Some(*wait),
                ..Self::default()
            },
            MachinePrinterOperation::SetChamberLight(on) => Self {
                light_on: Some(*on),
                ..Self::default()
            },
            MachinePrinterOperation::AmsRereadRfid { ams_id, slot_id } => Self {
                ams_id: Some(*ams_id),
                slot_id: Some(*slot_id),
                ..Self::default()
            },
            MachinePrinterOperation::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            }
            | MachinePrinterOperation::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self {
                ams_id: Some(*ams_id),
                slot_id: Some(*slot_id),
                global_tray_id: *global_tray_id,
                external_id: external_id.clone(),
                extruder_id: *extruder_id,
                ..Self::default()
            },
            MachinePrinterOperation::Pause
            | MachinePrinterOperation::Resume
            | MachinePrinterOperation::Stop
            | MachinePrinterOperation::ToggleLight => Self::default(),
        }
    }
}

impl MqttReportSummary {
    fn from_report(report: &Value) -> Option<Self> {
        let report = serde_json::from_value::<MqttReport>(report.clone()).ok()?;
        let section = report.print.or(report.system)?;
        Some(Self {
            result: section.result,
            reason: section.reason,
            err_code: section.err_code,
            errno: section.errno,
        })
    }
}

fn axis_name(axis: &MachinePrinterAxis) -> &'static str {
    match axis {
        MachinePrinterAxis::X => "x",
        MachinePrinterAxis::Y => "y",
        MachinePrinterAxis::Z => "z",
    }
}
