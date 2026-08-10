use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct DiscoverPrintersRequest<T> {
    timeout_seconds: T,
}

#[derive(Serialize)]
struct EmptyRequest {}

#[derive(Serialize)]
struct WebPrintErrorRequest<'a> {
    action: &'static str,
    error_action: &'a str,
    error_generation: u64,
}

#[derive(Serialize)]
struct DiagnosePrinterRequest<'a> {
    serial_number: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_code: Option<&'a str>,
}

#[derive(Serialize)]
struct PrinterDiscoveryResult<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    printers: Vec<DiscoveredPrinter<'a>>,
}

#[derive(Serialize)]
struct DiscoveredPrinter<'a> {
    serial_number: &'a str,
    host: &'a str,
    name: &'a str,
    model: &'a str,
    source: &'a str,
}

#[derive(Serialize)]
pub(super) struct PrinterControlRequest<'a> {
    action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_mode: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fan_index: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    airduct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_command: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    movements: Option<Vec<MoveAxisRequest<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    axes: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedrate_mm_per_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature_celsius: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    light_on: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    holder_action: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nozzle_id: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct MoveAxisRequest<'a> {
    axis: &'a str,
    delta_mm: f64,
}

impl<'a> PrinterControlRequest<'a> {
    pub(super) fn action(action: &'a str) -> Self {
        Self {
            action,
            speed_mode: None,
            fan_index: None,
            speed_percent: None,
            airduct: None,
            extruder_id: None,
            raw_command: None,
            movements: None,
            axes: None,
            feedrate_mm_per_min: None,
            temperature_celsius: None,
            wait: None,
            light_on: None,
            holder_action: None,
            nozzle_id: None,
        }
    }

    pub(super) fn set_print_speed(speed_mode: u8) -> Self {
        Self::action("set_print_speed").with_speed_mode(speed_mode)
    }

    pub(super) fn set_fan_speed(fan_index: u8, speed_percent: u8, airduct: bool) -> Self {
        let mut request = Self::action("set_fan_speed");
        request.fan_index = Some(fan_index);
        request.speed_percent = Some(speed_percent);
        request.airduct = Some(airduct);
        request
    }

    pub(super) fn select_extruder(extruder_id: u32) -> Self {
        let mut request = Self::action("select_extruder");
        request.extruder_id = Some(extruder_id);
        request
    }

    pub(super) fn home(axes: Vec<&'a str>) -> Self {
        let mut request = Self::action("home");
        request.axes = Some(axes);
        request
    }

    pub(super) fn move_axes(
        movements: Vec<MoveAxisRequest<'a>>,
        feedrate_mm_per_min: Option<u32>,
    ) -> Self {
        let mut request = Self::action("move_axes");
        request.movements = Some(movements);
        request.feedrate_mm_per_min = feedrate_mm_per_min;
        request
    }

    pub(super) fn set_temperature(action: &'a str, temperature_celsius: i32) -> Self {
        let mut request = Self::action(action);
        request.temperature_celsius = Some(temperature_celsius);
        request
    }

    pub(super) fn set_hotend_temperature(
        temperature_celsius: i32,
        wait: Option<bool>,
        extruder_id: Option<u32>,
    ) -> Self {
        let mut request = Self::set_temperature("set_hotend_temperature", temperature_celsius);
        request.wait = wait;
        request.extruder_id = extruder_id;
        request
    }

    pub(super) fn set_chamber_light(light_on: bool) -> Self {
        let mut request = Self::action("set_chamber_light");
        request.light_on = Some(light_on);
        request
    }

    pub(super) fn nozzle_holder_ctrl(holder_action: u32) -> Self {
        let mut request = Self::action("nozzle_holder_ctrl");
        request.holder_action = Some(holder_action);
        request
    }

    pub(super) fn rack_nozzle_operation(action: &'a str, nozzle_id: u32) -> Self {
        let mut request = Self::action(action);
        request.nozzle_id = Some(nozzle_id);
        request
    }

    pub(super) fn with_nozzle_id(mut self, nozzle_id: u32) -> Self {
        self.nozzle_id = Some(nozzle_id);
        self
    }

    pub(super) fn with_speed_mode(mut self, speed_mode: u8) -> Self {
        self.speed_mode = Some(speed_mode);
        self
    }

    pub(super) fn with_raw_command(mut self, raw_command: &'a str) -> Self {
        self.raw_command = Some(raw_command);
        self
    }
}

pub(super) fn move_axis(axis: &str, delta_mm: f64) -> MoveAxisRequest<'_> {
    MoveAxisRequest { axis, delta_mm }
}

pub(super) fn discover_printers_body(timeout_seconds: u64) -> Option<Value> {
    Some(value(DiscoverPrintersRequest { timeout_seconds }))
}

pub(super) fn discover_printers_timeout_string_body(timeout_seconds: &str) -> Option<Value> {
    Some(value(DiscoverPrintersRequest { timeout_seconds }))
}

pub(super) fn empty_body() -> Option<Value> {
    Some(value(EmptyRequest {}))
}

pub(super) fn diagnose_printer_body(serial_number: &str) -> Option<Value> {
    Some(value(DiagnosePrinterRequest {
        serial_number,
        access_code: None,
    }))
}

pub(super) fn diagnose_printer_with_access_code_body(
    serial_number: &str,
    access_code: &str,
) -> Option<Value> {
    Some(value(DiagnosePrinterRequest {
        serial_number,
        access_code: Some(access_code),
    }))
}

pub(super) fn printer_control_body(request: PrinterControlRequest<'_>) -> Option<Value> {
    Some(printer_control_value(request))
}

pub(super) fn printer_control_value(request: PrinterControlRequest<'_>) -> Value {
    value(request)
}

pub(super) fn web_print_error_body(error_action: &str, error_generation: u64) -> Option<Value> {
    Some(value(WebPrintErrorRequest {
        action: "handle_print_error",
        error_action,
        error_generation,
    }))
}

pub(super) fn printer_discovery_result_json() -> String {
    value(PrinterDiscoveryResult {
        kind: "printer_discovery",
        printers: vec![DiscoveredPrinter {
            serial_number: "BAMBU123",
            host: "192.0.2.10",
            name: "Shop A1",
            model: "A1",
            source: "ssdp",
        }],
    })
    .to_string()
}

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
