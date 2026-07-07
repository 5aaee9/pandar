use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct ExpectedInfoPayload<'a> {
    info: ExpectedInfo<'a>,
}

#[derive(Serialize)]
struct ExpectedInfo<'a> {
    command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    module: Vec<ExpectedGetVersionModule<'a>>,
}

#[derive(Serialize)]
struct ExpectedGetVersionModule<'a> {
    name: &'static str,
    product_name: &'a str,
}

#[derive(Serialize)]
struct ExpectedPushingPayload<'a> {
    pushing: ExpectedPushing<'a>,
}

#[derive(Serialize)]
struct ExpectedPushing<'a> {
    command: &'static str,
    sequence_id: &'a str,
    version: u8,
    push_target: u8,
}

#[derive(Serialize)]
struct ExpectedPrintPayload<'a> {
    print: ExpectedPrint<'a>,
}

#[derive(Serialize)]
struct ExpectedPrint<'a> {
    command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    param: Option<&'a str>,
    sequence_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_index: Option<u32>,
}

#[derive(Serialize)]
struct ExpectedSystemPayload<'a> {
    system: ExpectedSystem<'a>,
}

#[derive(Serialize)]
struct PrintStateReport<'a> {
    print: PrintState<'a>,
}

#[derive(Serialize)]
struct PrintState<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    gcode_state: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'a str>,
}

#[derive(Serialize)]
struct ExpectedSystem<'a> {
    command: &'static str,
    led_node: &'static str,
    led_mode: &'static str,
    led_on_time: u32,
    led_off_time: u32,
    loop_times: u32,
    interval_time: u32,
    sequence_id: &'a str,
}

pub(super) fn expected_pushall_payload(sequence_id: &str) -> Value {
    value(ExpectedPushingPayload {
        pushing: ExpectedPushing {
            command: "pushall",
            sequence_id,
            version: 1,
            push_target: 1,
        },
    })
}

pub(super) fn expected_get_version_payload(sequence_id: &str) -> Value {
    value(ExpectedInfoPayload {
        info: ExpectedInfo {
            command: "get_version",
            sequence_id: Some(sequence_id),
            module: Vec::new(),
        },
    })
}

pub(super) fn get_version_report_with_blank_model() -> Value {
    value(ExpectedInfoPayload {
        info: ExpectedInfo {
            command: "get_version",
            sequence_id: None,
            module: vec![ExpectedGetVersionModule {
                name: "ota",
                product_name: "   ",
            }],
        },
    })
}

pub(super) fn info_command_report(command: &'static str) -> Value {
    value(ExpectedInfoPayload {
        info: ExpectedInfo {
            command,
            sequence_id: None,
            module: Vec::new(),
        },
    })
}

pub(super) fn print_gcode_state_report(gcode_state: &str) -> Value {
    value(PrintStateReport {
        print: PrintState {
            gcode_state: Some(gcode_state),
            state: None,
        },
    })
}

pub(super) fn print_state_report(state: &str) -> Value {
    value(PrintStateReport {
        print: PrintState {
            gcode_state: None,
            state: Some(state),
        },
    })
}

pub(super) fn expected_print_command_payload(
    command: &'static str,
    param: &str,
    sequence_id: &str,
) -> Value {
    value(ExpectedPrintPayload {
        print: ExpectedPrint {
            command,
            param: Some(param),
            sequence_id,
            extruder_index: None,
        },
    })
}

pub(super) fn expected_select_extruder_payload(extruder_index: u32, sequence_id: &str) -> Value {
    value(ExpectedPrintPayload {
        print: ExpectedPrint {
            command: "select_extruder",
            param: None,
            sequence_id,
            extruder_index: Some(extruder_index),
        },
    })
}

pub(super) fn expected_chamber_light_payload(sequence_id: &str) -> Value {
    value(ExpectedSystemPayload {
        system: ExpectedSystem {
            command: "ledctrl",
            led_node: "chamber_light",
            led_mode: "on",
            led_on_time: 500,
            led_off_time: 500,
            loop_times: 1,
            interval_time: 1000,
            sequence_id,
        },
    })
}

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
