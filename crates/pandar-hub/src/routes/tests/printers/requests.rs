use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct UpdatePrinterRequest<'a> {
    host: &'a str,
    access_code: &'a str,
    name: &'a str,
}

#[derive(Serialize)]
struct PrinterAmsLoadRequest {
    action: &'static str,
    ams_id: u32,
    slot_id: u32,
    global_tray_id: u32,
    extruder_id: u32,
}

#[derive(Serialize)]
struct PrinterSelectExtruderRequest {
    action: &'static str,
    extruder_id: u32,
}

#[derive(Serialize)]
struct LinkPrinterRequest<'a> {
    #[serde(rename = "type")]
    printer_type: &'a str,
    host: &'a str,
    access_code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

#[derive(Serialize)]
struct LinkPrinterWithSerialNumberRequest<'a> {
    #[serde(rename = "type")]
    printer_type: &'a str,
    host: &'a str,
    access_code: &'a str,
    serial_number: &'a str,
}

#[derive(Serialize)]
struct LinkPrinterWithModelRequest<'a> {
    #[serde(rename = "type")]
    printer_type: &'a str,
    host: &'a str,
    access_code: &'a str,
    model: &'a str,
}

#[derive(Serialize)]
struct LinkPrinterWithUnexpectedFieldRequest<'a> {
    #[serde(rename = "type")]
    printer_type: &'a str,
    host: &'a str,
    access_code: &'a str,
    unexpected: bool,
}

pub(super) fn update_printer_body(host: &str, access_code: &str, name: &str) -> Option<Value> {
    Some(value(UpdatePrinterRequest {
        host,
        access_code,
        name,
    }))
}

pub(super) fn printer_ams_load_body(
    ams_id: u32,
    slot_id: u32,
    global_tray_id: u32,
    extruder_id: u32,
) -> Option<Value> {
    Some(value(PrinterAmsLoadRequest {
        action: "ams_load_filament",
        ams_id,
        slot_id,
        global_tray_id,
        extruder_id,
    }))
}

pub(super) fn printer_select_extruder_body(extruder_id: u32) -> Option<Value> {
    Some(value(PrinterSelectExtruderRequest {
        action: "select_extruder",
        extruder_id,
    }))
}

pub(super) fn link_printer_body(access_code: &str) -> Value {
    link_printer_value("BambuLab", "192.0.2.10", access_code, Some("Office X1C"))
}

pub(super) fn link_printer_value(
    printer_type: &str,
    host: &str,
    access_code: &str,
    name: Option<&str>,
) -> Value {
    value(LinkPrinterRequest {
        printer_type,
        host,
        access_code,
        name,
    })
}

pub(super) fn link_printer_with_serial_number_value(
    printer_type: &str,
    host: &str,
    access_code: &str,
    serial_number: &str,
) -> Value {
    value(LinkPrinterWithSerialNumberRequest {
        printer_type,
        host,
        access_code,
        serial_number,
    })
}

pub(super) fn link_printer_with_model_value(
    printer_type: &str,
    host: &str,
    access_code: &str,
    model: &str,
) -> Value {
    value(LinkPrinterWithModelRequest {
        printer_type,
        host,
        access_code,
        model,
    })
}

pub(super) fn link_printer_with_unexpected_field_body(
    printer_type: &str,
    host: &str,
    access_code: &str,
) -> Option<Value> {
    Some(value(LinkPrinterWithUnexpectedFieldRequest {
        printer_type,
        host,
        access_code,
        unexpected: true,
    }))
}

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
