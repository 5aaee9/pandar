use serde_json::json;

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
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), json!("printer_operation"));
    result.insert(
        "action".to_string(),
        json!(printer_operation_action(operation)),
    );
    result.insert("serial_number".to_string(), json!(serial_number));
    if let Some(sequence_id) = &dispatch_result.sequence_id {
        result.insert("sequence_id".to_string(), json!(sequence_id));
    }
    if let Some(error) = &dispatch_result.error {
        result.insert("mqtt_error".to_string(), json!(error));
    }
    if let Some(report) = &dispatch_result.mqtt_report {
        if let Some(section) = report.get("print").or_else(|| report.get("system")) {
            if let Some(value) = section.get("result") {
                result.insert("mqtt_result".to_string(), value.clone());
            }
            if let Some(value) = section.get("reason") {
                result.insert("mqtt_reason".to_string(), value.clone());
            }
            if let Some(value) = section.get("err_code") {
                result.insert("mqtt_err_code".to_string(), value.clone());
            }
            if let Some(value) = section.get("errno") {
                result.insert("mqtt_errno".to_string(), value.clone());
            }
        }
        result.insert("mqtt_report".to_string(), report.clone());
    }
    append_operation_fields(&mut result, operation);
    serde_json::Value::Object(result).to_string()
}

fn append_operation_fields(
    result: &mut serde_json::Map<String, serde_json::Value>,
    operation: &MachinePrinterOperation,
) {
    match operation {
        MachinePrinterOperation::SetPrintSpeed(speed_mode) => {
            result.insert("speed_mode".to_string(), json!(speed_mode));
        }
        MachinePrinterOperation::SelectExtruder(extruder_id) => {
            result.insert("extruder_id".to_string(), json!(extruder_id));
        }
        MachinePrinterOperation::Home { axes } => {
            result.insert(
                "axes".to_string(),
                json!(
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
                result.insert("x_mm".to_string(), json!(value));
            }
            if let Some(value) = y_mm {
                result.insert("y_mm".to_string(), json!(value));
            }
            if let Some(value) = z_mm {
                result.insert("z_mm".to_string(), json!(value));
            }
            if let Some(value) = feedrate_mm_per_min {
                result.insert("feedrate_mm_per_min".to_string(), json!(value));
            }
        }
        MachinePrinterOperation::SetHotendTemperature {
            temperature_celsius,
            wait,
            extruder_id,
        } => {
            result.insert(
                "temperature_celsius".to_string(),
                json!(temperature_celsius),
            );
            result.insert("wait".to_string(), json!(wait));
            if let Some(value) = extruder_id {
                result.insert("extruder_id".to_string(), json!(value));
            }
        }
        MachinePrinterOperation::SetBedTemperature {
            temperature_celsius,
            wait,
        }
        | MachinePrinterOperation::SetChamberTemperature {
            temperature_celsius,
            wait,
        } => {
            result.insert(
                "temperature_celsius".to_string(),
                json!(temperature_celsius),
            );
            result.insert("wait".to_string(), json!(wait));
        }
        MachinePrinterOperation::SetChamberLight(on) => {
            result.insert("light_on".to_string(), json!(on));
        }
        MachinePrinterOperation::AmsRereadRfid { ams_id, slot_id } => {
            result.insert("ams_id".to_string(), json!(ams_id));
            result.insert("slot_id".to_string(), json!(slot_id));
        }
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
        } => {
            result.insert("ams_id".to_string(), json!(ams_id));
            result.insert("slot_id".to_string(), json!(slot_id));
            result.insert("global_tray_id".to_string(), json!(global_tray_id));
            result.insert("external_id".to_string(), json!(external_id));
            result.insert("extruder_id".to_string(), json!(extruder_id));
        }
        MachinePrinterOperation::Pause
        | MachinePrinterOperation::Resume
        | MachinePrinterOperation::Stop
        | MachinePrinterOperation::ToggleLight => {}
    }
}
