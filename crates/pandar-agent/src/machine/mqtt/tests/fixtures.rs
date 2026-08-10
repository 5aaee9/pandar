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
struct RawPrintPayload<'a> {
    print: RawPrint<'a>,
}

#[derive(Serialize)]
struct RawPrint<'a> {
    command: &'a str,
    sequence_id: &'a str,
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
struct PrintDetailedProgressReport<'a> {
    print: PrintDetailedProgress<'a>,
}

#[derive(Serialize)]
struct PrintDetailedProgress<'a> {
    task_id: &'a str,
    subtask_id: &'a str,
    gcode_state: &'a str,
    mc_percent: &'a str,
    mc_remaining_time: u32,
    spd_lvl: u8,
    layer_num: &'a str,
    total_layer_num: u32,
    gcode_file: &'a str,
    subtask_name: &'a str,
    print_error: &'a str,
    hms: Vec<PrintHmsDiagnostic<'a>>,
}

#[derive(Serialize)]
struct PrintHmsDiagnostic<'a> {
    code: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct PrintOutOfRangeProgressReport<'a> {
    print: PrintOutOfRangeProgress<'a>,
}

#[derive(Serialize)]
struct PrintOutOfRangeProgress<'a> {
    mc_percent: &'a str,
    mc_remaining_time: u32,
    layer_num: &'a str,
    total_layer_num: i32,
}

#[derive(Serialize)]
struct PrintJobProgressReport<'a> {
    print: PrintJobProgress<'a>,
}

#[derive(Serialize)]
struct PrintJobProgress<'a> {
    task_id: &'a str,
    subtask_id: &'a str,
    gcode_state: &'a str,
    mc_percent: u32,
}

#[derive(Serialize)]
struct PrintTemperatureReport<'a> {
    print: PrintTemperature<'a>,
}

#[derive(Serialize)]
struct PrintTemperature<'a> {
    gcode_state: &'a str,
    nozzle_temper: u32,
    nozzle_target_temper: u32,
    bed_temper: u32,
    chamber_temper: u32,
}

#[derive(Serialize)]
struct ChamberTargetReport {
    print: ChamberTarget,
}

#[derive(Serialize)]
struct ChamberTarget {
    ctt: u32,
}

#[derive(Serialize)]
struct AmsPrintReport<'a> {
    print: AmsPrint<'a>,
}

#[derive(Serialize)]
struct AmsPrint<'a> {
    gcode_state: &'a str,
    ams: AmsReport<'a>,
}

#[derive(Serialize)]
struct AmsReport<'a> {
    ams: Vec<AmsUnit<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_now: Option<&'a str>,
}

#[derive(Serialize)]
struct AmsUnit<'a> {
    id: &'a str,
    tray: Vec<AmsTray<'a>>,
}

#[derive(Serialize)]
struct AmsTray<'a> {
    id: &'a str,
    tray_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_color: Option<&'a str>,
}

#[derive(Serialize)]
struct ExternalVtTrayReport<'a> {
    print: ExternalVtTrayPrint<'a>,
}

#[derive(Serialize)]
struct ExternalVtTrayPrint<'a> {
    ams: ExternalVtTrayAms<'a>,
}

#[derive(Serialize)]
struct ExternalVtTrayAms<'a> {
    tray_now: u32,
    vt_tray: ExternalVtTray<'a>,
}

#[derive(Serialize)]
struct ExternalVtTray<'a> {
    tray_info_idx: &'a str,
    tray_color: &'a str,
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

pub(super) fn detailed_progress_report() -> Value {
    value(PrintDetailedProgressReport {
        print: PrintDetailedProgress {
            task_id: "job-123",
            subtask_id: "artifact-456",
            gcode_state: "RUNNING",
            mc_percent: "42",
            mc_remaining_time: 87,
            spd_lvl: 3,
            layer_num: "12",
            total_layer_num: 120,
            gcode_file: "plate_1.gcode",
            subtask_name: "drawer-organizer",
            print_error: "nozzle temperature error",
            hms: vec![PrintHmsDiagnostic {
                code: "0300_0A00_0001_0002",
                message: "fan speed is low",
            }],
        },
    })
}

pub(super) fn out_of_range_progress_report() -> Value {
    value(PrintOutOfRangeProgressReport {
        print: PrintOutOfRangeProgress {
            mc_percent: "101",
            mc_remaining_time: 4321,
            layer_num: "100001",
            total_layer_num: -1,
        },
    })
}

pub(super) fn print_job_progress_report(
    task_id: &str,
    subtask_id: &str,
    gcode_state: &str,
    mc_percent: u32,
) -> Value {
    value(PrintJobProgressReport {
        print: PrintJobProgress {
            task_id,
            subtask_id,
            gcode_state,
            mc_percent,
        },
    })
}

pub(super) fn print_temperature_report(
    gcode_state: &str,
    nozzle_temper: u32,
    nozzle_target_temper: u32,
    bed_temper: u32,
    chamber_temper: u32,
) -> Value {
    value(PrintTemperatureReport {
        print: PrintTemperature {
            gcode_state,
            nozzle_temper,
            nozzle_target_temper,
            bed_temper,
            chamber_temper,
        },
    })
}

pub(super) fn chamber_target_report(ctt: u32) -> Value {
    value(ChamberTargetReport {
        print: ChamberTarget { ctt },
    })
}

pub(super) fn ams_print_report(
    gcode_state: &str,
    tray_type: &str,
    tray_color: Option<&str>,
    tray_now: Option<&str>,
) -> Value {
    value(AmsPrintReport {
        print: AmsPrint {
            gcode_state,
            ams: AmsReport {
                ams: vec![AmsUnit {
                    id: "0",
                    tray: vec![AmsTray {
                        id: "0",
                        tray_type,
                        tray_color,
                    }],
                }],
                tray_now,
            },
        },
    })
}

pub(super) fn external_vt_tray_report(
    tray_now: u32,
    tray_info_idx: &str,
    tray_color: &str,
) -> Value {
    value(ExternalVtTrayReport {
        print: ExternalVtTrayPrint {
            ams: ExternalVtTrayAms {
                tray_now,
                vt_tray: ExternalVtTray {
                    tray_info_idx,
                    tray_color,
                },
            },
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

pub(super) fn raw_print_payload(command: &str, sequence_id: &str) -> Value {
    value(RawPrintPayload {
        print: RawPrint {
            command,
            sequence_id,
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
