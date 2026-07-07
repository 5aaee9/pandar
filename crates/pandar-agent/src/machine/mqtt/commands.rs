use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::bail;
use serde_json::Value;

mod payload;

use payload::*;

const STUDIO_START_SEQUENCE_ID: u32 = 20000;
const STUDIO_END_SEQUENCE_ID: u32 = 30000;
static STUDIO_SEQUENCE_ID: AtomicU32 = AtomicU32::new(STUDIO_START_SEQUENCE_ID);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BambuMqttTopics {
    pub report: String,
    pub request: String,
}

impl BambuMqttTopics {
    pub fn for_serial(serial: &str) -> Self {
        Self {
            report: format!("device/{serial}/report"),
            request: format!("device/{serial}/request"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintSpeed(u8);

impl PrintSpeed {
    pub fn new(mode: u8) -> anyhow::Result<Self> {
        if !(1..=4).contains(&mode) {
            bail!("invalid Bambu print speed mode {mode}; expected 1..=4");
        }

        Ok(Self(mode))
    }

    pub fn as_u8(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileCommand {
    pub filename: String,
    pub url: Option<String>,
    pub md5: Option<String>,
    pub plate_id: u32,
    pub task_id: String,
    pub subtask_id: String,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
    pub ams_mapping_info_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcodeLineCommand {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetNozzleTemperatureCommand {
    pub extruder_id: u32,
    pub target_temp: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmsSlotCommand {
    pub ams_id: u32,
    pub slot_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmsFilamentCommand {
    pub ams_id: u32,
    pub slot_id: u32,
    pub target: u32,
    pub extruder_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintReportProgress {
    pub serial: String,
    pub job_id: Option<String>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub diagnostics: Vec<MachineReportDiagnostic>,
    pub observed_at: String,
    pub printer_materials_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineReportDiagnostic {
    pub kind: String,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BambuMqttCommand {
    GetVersion,
    RequestPushAll,
    PausePrint,
    ResumePrint,
    StopPrint,
    SetChamberLight(bool),
    SetPrintSpeed(PrintSpeed),
    SelectExtruder(u32),
    SetNozzleTemperature(SetNozzleTemperatureCommand),
    GcodeLine(GcodeLineCommand),
    AmsRereadRfid(AmsSlotCommand),
    AmsLoadFilament(AmsFilamentCommand),
    AmsUnloadFilament(AmsFilamentCommand),
    RawJson(Value),
    ProjectFile(ProjectFileCommand),
}

impl BambuMqttCommand {
    pub fn payload(&self) -> Value {
        match self {
            Self::GetVersion => info_payload("get_version"),
            Self::RequestPushAll => pushing_payload(),
            Self::PausePrint => basic_print_payload("pause"),
            Self::ResumePrint => basic_print_payload("resume"),
            Self::StopPrint => basic_print_payload("stop"),
            Self::SetChamberLight(on) => chamber_light_payload("chamber_light", *on),
            Self::SetPrintSpeed(speed) => print_speed_payload(*speed),
            Self::SelectExtruder(extruder_id) => select_extruder_payload(*extruder_id),
            Self::SetNozzleTemperature(command) => set_nozzle_temperature_payload(command),
            Self::GcodeLine(command) => gcode_line_payload(command),
            Self::AmsRereadRfid(command) => ams_reread_rfid_payload(command),
            Self::AmsLoadFilament(command) => ams_load_filament_payload(command),
            Self::AmsUnloadFilament(command) => ams_unload_filament_payload(command),
            Self::RawJson(payload) => payload.clone(),
            Self::ProjectFile(command) => project_file_payload(command),
        }
    }
}

fn info_payload(command: &'static str) -> Value {
    json_payload(InfoPayload {
        info: InfoCommand {
            command,
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn pushing_payload() -> Value {
    json_payload(PushingPayload {
        pushing: PushingCommand {
            command: "pushall",
            sequence_id: next_studio_sequence_id(),
            version: 1,
            push_target: 1,
        },
    })
}

fn basic_print_payload(command: &'static str) -> Value {
    json_payload(PrintPayload {
        print: BasicPrintCommand {
            command,
            param: "",
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn print_speed_payload(speed: PrintSpeed) -> Value {
    json_payload(PrintPayload {
        print: PrintSpeedCommand {
            command: "print_speed",
            param: speed.as_u8().to_string(),
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn select_extruder_payload(extruder_id: u32) -> Value {
    json_payload(PrintPayload {
        print: SelectExtruderCommand {
            command: "select_extruder",
            extruder_index: extruder_id,
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn set_nozzle_temperature_payload(command: &SetNozzleTemperatureCommand) -> Value {
    json_payload(PrintPayload {
        print: SetNozzleTemperaturePayload {
            command: "set_nozzle_temp",
            extruder_index: command.extruder_id,
            target_temp: command.target_temp,
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn gcode_line_payload(command: &GcodeLineCommand) -> Value {
    json_payload(PrintPayload {
        print: GcodeLinePayload {
            command: "gcode_line",
            param: command.lines.join("\n"),
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn next_studio_sequence_id() -> String {
    next_studio_sequence_id_from(&STUDIO_SEQUENCE_ID)
}

pub(crate) fn next_studio_sequence_id_from(sequence: &AtomicU32) -> String {
    loop {
        let current = sequence.load(Ordering::Relaxed);
        let sequence_id = if (STUDIO_START_SEQUENCE_ID..STUDIO_END_SEQUENCE_ID).contains(&current) {
            current
        } else {
            STUDIO_START_SEQUENCE_ID
        };
        let next = if sequence_id + 1 >= STUDIO_END_SEQUENCE_ID {
            STUDIO_START_SEQUENCE_ID
        } else {
            sequence_id + 1
        };

        if sequence
            .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return sequence_id.to_string();
        }
    }
}

fn ams_reread_rfid_payload(command: &AmsSlotCommand) -> Value {
    json_payload(PrintPayload {
        print: AmsSlotPayload {
            command: "ams_get_rfid",
            sequence_id: next_studio_sequence_id(),
            ams_id: command.ams_id,
            slot_id: command.slot_id,
        },
    })
}

pub(crate) fn chamber_light_payloads_for_nodes<'a>(
    nodes: impl IntoIterator<Item = &'a str>,
    on: bool,
) -> Vec<Value> {
    nodes
        .into_iter()
        .map(|node| chamber_light_payload(node, on))
        .collect()
}

fn chamber_light_payload(node: &str, on: bool) -> Value {
    json_payload(SystemPayload {
        system: ChamberLightPayload {
            command: "ledctrl",
            led_node: node,
            led_mode: if on { "on" } else { "off" },
            led_on_time: 500,
            led_off_time: 500,
            loop_times: 1,
            interval_time: 1000,
            sequence_id: next_studio_sequence_id(),
        },
    })
}

fn ams_load_filament_payload(command: &AmsFilamentCommand) -> Value {
    json_payload(PrintPayload {
        print: AmsChangeFilamentPayload {
            command: "ams_change_filament",
            sequence_id: next_studio_sequence_id(),
            ams_id: command.ams_id,
            slot_id: command.slot_id,
            target: command.target,
            curr_temp: -1,
            tar_temp: -1,
            extruder_id: command.extruder_id,
        },
    })
}

fn ams_unload_filament_payload(command: &AmsFilamentCommand) -> Value {
    let _ = command.slot_id;
    let _ = command.target;
    json_payload(PrintPayload {
        print: AmsChangeFilamentPayload {
            command: "ams_change_filament",
            sequence_id: next_studio_sequence_id(),
            ams_id: command.ams_id,
            slot_id: 255,
            target: 255,
            curr_temp: 210,
            tar_temp: 210,
            extruder_id: None,
        },
    })
}

fn project_file_payload(command: &ProjectFileCommand) -> Value {
    json_payload(ProjectFilePayload {
        print: ProjectFilePayloadPrint {
            command: "project_file",
            sequence_id: next_studio_sequence_id(),
            param: format!("Metadata/plate_{}.gcode", command.plate_id),
            project_id: "0",
            profile_id: "0",
            task_id: "0",
            subtask_id: "0",
            subtask_name: project_file_subtask_name(&command.filename),
            url: command
                .url
                .clone()
                .unwrap_or_else(|| format!("ftp://{}", command.filename)),
            file: command.filename.clone(),
            md5: command.md5.clone().unwrap_or_default(),
            bed_type: "auto",
            bed_leveling: false,
            flow_cali: command.flow_cali,
            vibration_cali: false,
            layer_inspect: false,
            timelapse: command.timelapse,
            use_ams: command.use_ams,
            ams_mapping: command
                .ams_mapping_json
                .as_deref()
                .and_then(project_file_ams_mapping)
                .unwrap_or_default(),
            ams_mapping2: command
                .ams_mapping2_json
                .as_deref()
                .and_then(project_file_mapping_array)
                .unwrap_or_default(),
            ams_mapping_info: command
                .ams_mapping_info_json
                .as_deref()
                .and_then(project_file_mapping_array),
            auto_bed_leveling: 0,
            nozzle_offset_cali: 0,
            cfg: "0",
            extrude_cali_flag: 0,
        },
    })
}

fn project_file_subtask_name(filename: &str) -> String {
    let base = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();
    let stem = base
        .strip_suffix(".gcode.3mf")
        .or_else(|| base.strip_suffix(".3mf"))
        .unwrap_or(base)
        .trim();
    if stem.is_empty() {
        "print".to_string()
    } else {
        stem.to_string()
    }
}

fn project_file_ams_mapping(raw: &str) -> Option<Vec<Value>> {
    Some(
        project_file_mapping_array(raw)?
            .into_iter()
            .map(|value| match value.as_i64() {
                Some(254 | 255) => Value::from(-1),
                _ => value,
            })
            .collect(),
    )
}

fn project_file_mapping_array(raw: &str) -> Option<Vec<Value>> {
    serde_json::from_str(raw).ok()
}
