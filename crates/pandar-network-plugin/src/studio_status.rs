mod device;
mod input;
mod materials;
mod scalar;

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
