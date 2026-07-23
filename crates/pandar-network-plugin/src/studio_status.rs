mod capabilities;
mod device;
mod firmware;
mod input;
mod list;
mod materials;
mod payload;
mod request;
mod scalar;

pub(super) use firmware::{
    acknowledgement_callback_json, current_firmware_json, firmware_refresh_failure_json,
    firmware_refresh_success_json, firmware_reset_json,
};
pub(super) use list::{PrinterObservation, firmware_observations, printer_observations};

use device::StudioTelemetry;
use input::PrinterStatus;

pub fn printer_telemetry_fragment(printer_json: &str) -> String {
    let printer = serde_json::from_str::<PrinterStatus>(printer_json).unwrap_or_default();
    let telemetry = StudioTelemetry::from(&printer);
    let object = serde_json::to_string(&telemetry).expect("studio telemetry is serializable");
    object
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(&object)
        .to_string()
}

pub(super) fn local_connect_json(dev_id: &str, model: &str) -> String {
    payload::local_connect_json(dev_id, model)
}

pub(super) fn classify_status_request(message: &str) -> (i32, String) {
    request::classify_status_request(message)
}

pub(crate) use request::{StudioStatusRequest, parse_status_request};
