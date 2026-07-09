use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct PrintStateReport<'a> {
    print: PrintState<'a>,
}

#[derive(Serialize)]
struct PrintState<'a> {
    state: &'a str,
}

#[derive(Serialize)]
struct RootStateReport<'a> {
    state: &'a str,
}

#[derive(Serialize)]
struct InfoPayload<'a> {
    info: Info<'a>,
}

#[derive(Serialize)]
struct Info<'a> {
    command: &'static str,
    sequence_id: &'a str,
}

#[derive(Serialize)]
struct PushingPayload<'a> {
    pushing: Pushing<'a>,
}

#[derive(Serialize)]
struct Pushing<'a> {
    command: &'static str,
    sequence_id: &'a str,
    version: u8,
    push_target: u8,
}

#[derive(Serialize)]
struct ProjectFilePayload<'a> {
    print: ProjectFile<'a>,
}

#[derive(Serialize)]
struct ProjectFile<'a> {
    command: &'static str,
    sequence_id: &'a str,
    param: &'static str,
    project_id: &'a str,
    profile_id: &'static str,
    task_id: &'a str,
    subtask_id: &'a str,
    subtask_name: &'static str,
    url: &'static str,
    file: &'static str,
    md5: &'static str,
    bed_type: &'static str,
    bed_leveling: bool,
    flow_cali: bool,
    vibration_cali: bool,
    layer_inspect: bool,
    timelapse: bool,
    use_ams: bool,
    ams_mapping: Vec<Value>,
    ams_mapping2: Vec<Value>,
    auto_bed_leveling: u8,
    nozzle_offset_cali: u8,
    cfg: &'static str,
    extrude_cali_flag: u8,
}

#[derive(Serialize)]
struct PrintCommandPayload<'a> {
    print: PrintCommand<'a>,
}

#[derive(Serialize)]
struct PrintCommand<'a> {
    command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    param: Option<&'a str>,
    sequence_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_index: Option<u32>,
}

#[derive(Serialize)]
struct TargetedHotendPayload<'a> {
    print: TargetedHotend<'a>,
}

#[derive(Serialize)]
struct TargetedHotend<'a> {
    command: &'static str,
    extruder_index: u32,
    target_temp: u32,
    sequence_id: &'a str,
}

#[derive(Serialize)]
struct AmsChangeFilamentPayload<'a> {
    print: AmsChangeFilament<'a>,
}

#[derive(Serialize)]
struct AmsChangeFilament<'a> {
    command: &'static str,
    sequence_id: &'a str,
    ams_id: u32,
    slot_id: u32,
    target: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_id: Option<u32>,
    curr_temp: i32,
    tar_temp: i32,
}

#[derive(Serialize)]
struct AmsGetRfidPayload<'a> {
    print: AmsGetRfid<'a>,
}

#[derive(Serialize)]
struct AmsGetRfid<'a> {
    command: &'static str,
    sequence_id: &'a str,
    ams_id: u32,
    slot_id: u32,
}

#[derive(Serialize)]
struct LightsReport<'a> {
    print: Lights<'a>,
}

#[derive(Serialize)]
struct Lights<'a> {
    lights_report: Vec<LightState<'a>>,
}

#[derive(Serialize)]
struct LightState<'a> {
    node: &'a str,
    mode: &'a str,
}

#[derive(Serialize)]
struct SystemPayload<'a> {
    system: System<'a>,
}

#[derive(Serialize)]
struct System<'a> {
    command: &'static str,
    led_node: &'a str,
    led_mode: &'a str,
    led_on_time: u32,
    led_off_time: u32,
    loop_times: u32,
    interval_time: u32,
    sequence_id: &'a str,
}

pub(super) fn print_state_report(state: &str) -> Value {
    value(PrintStateReport {
        print: PrintState { state },
    })
}

pub(super) fn root_state_report(state: &str) -> Value {
    value(RootStateReport { state })
}

pub(super) fn expected_get_version_payload(sequence_id: &str) -> Value {
    value(InfoPayload {
        info: Info {
            command: "get_version",
            sequence_id,
        },
    })
}

pub(super) fn expected_pushall_payload(sequence_id: &str) -> Value {
    value(PushingPayload {
        pushing: Pushing {
            command: "pushall",
            sequence_id,
            version: 1,
            push_target: 1,
        },
    })
}

pub(super) fn expected_project_file_payload(sequence_id: &str, submission_id: &str) -> Value {
    value(ProjectFilePayload {
        print: ProjectFile {
            command: "project_file",
            sequence_id,
            param: "Metadata/plate_1.gcode",
            project_id: submission_id,
            profile_id: "0",
            task_id: submission_id,
            subtask_id: submission_id,
            subtask_name: "plate",
            url: "ftp://plate.gcode.3mf",
            file: "plate.gcode.3mf",
            md5: "900150983CD24FB0D6963F7D28E17F72",
            bed_type: "auto",
            bed_leveling: false,
            flow_cali: false,
            vibration_cali: false,
            layer_inspect: false,
            timelapse: true,
            use_ams: true,
            ams_mapping: Vec::new(),
            ams_mapping2: Vec::new(),
            auto_bed_leveling: 0,
            nozzle_offset_cali: 0,
            cfg: "0",
            extrude_cali_flag: 0,
        },
    })
}

pub(super) fn expected_print_command_payload(
    command: &'static str,
    param: &str,
    sequence_id: &str,
) -> Value {
    value(PrintCommandPayload {
        print: PrintCommand {
            command,
            param: Some(param),
            sequence_id,
            extruder_index: None,
        },
    })
}

pub(super) fn expected_select_extruder_payload(extruder_index: u32, sequence_id: &str) -> Value {
    value(PrintCommandPayload {
        print: PrintCommand {
            command: "select_extruder",
            param: None,
            sequence_id,
            extruder_index: Some(extruder_index),
        },
    })
}

pub(super) fn expected_targeted_hotend_payload(
    extruder_index: u32,
    target_temp: u32,
    sequence_id: &str,
) -> Value {
    value(TargetedHotendPayload {
        print: TargetedHotend {
            command: "set_nozzle_temp",
            extruder_index,
            target_temp,
            sequence_id,
        },
    })
}

pub(super) fn expected_ams_change_filament_payload(
    ams_id: u32,
    slot_id: u32,
    target: u32,
    extruder_id: Option<u32>,
    current_temp: i32,
    target_temp: i32,
    sequence_id: &str,
) -> Value {
    value(AmsChangeFilamentPayload {
        print: AmsChangeFilament {
            command: "ams_change_filament",
            sequence_id,
            ams_id,
            slot_id,
            target,
            extruder_id,
            curr_temp: current_temp,
            tar_temp: target_temp,
        },
    })
}

pub(super) fn expected_ams_get_rfid_payload(ams_id: u32, slot_id: u32, sequence_id: &str) -> Value {
    value(AmsGetRfidPayload {
        print: AmsGetRfid {
            command: "ams_get_rfid",
            sequence_id,
            ams_id,
            slot_id,
        },
    })
}

pub(super) fn lights_report(states: &[(&str, &str)]) -> Value {
    value(LightsReport {
        print: Lights {
            lights_report: states
                .iter()
                .map(|(node, mode)| LightState { node, mode })
                .collect(),
        },
    })
}

pub(super) fn expected_light_payload(node: &str, mode: &str, sequence_id: &str) -> Value {
    value(SystemPayload {
        system: System {
            command: "ledctrl",
            led_node: node,
            led_mode: mode,
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
