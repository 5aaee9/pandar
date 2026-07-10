use serde::Deserialize;

use crate::support::request_body;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum TestOperation {
    Home {
        axes: Vec<String>,
    },
    SetChamberLight {
        light_on: bool,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: u8,
    },
    HandlePrintError {
        error_action: TestPrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum TestPrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

pub(super) fn assert_operation_body_eq(request: &str, expected: TestOperation) {
    let actual: TestOperation = serde_json::from_str(request_body(request)).unwrap();
    assert_eq!(actual, expected);
}
