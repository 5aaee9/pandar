use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::bail;
use serde_json::{Value, json};

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
    pub plate_id: u32,
    pub task_id: String,
    pub subtask_id: String,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcodeLineCommand {
    pub lines: Vec<String>,
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
    SetPrintSpeed(PrintSpeed),
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
            Self::GetVersion => {
                json!({"info": {"command": "get_version", "sequence_id": next_studio_sequence_id()}})
            }
            Self::RequestPushAll => json!({"pushing": {
                "command": "pushall",
                "sequence_id": next_studio_sequence_id(),
                "version": 1,
                "push_target": 1
            }}),
            Self::PausePrint => {
                json!({"print": {"command": "pause", "param": "", "sequence_id": next_studio_sequence_id()}})
            }
            Self::ResumePrint => {
                json!({"print": {"command": "resume", "param": "", "sequence_id": next_studio_sequence_id()}})
            }
            Self::StopPrint => {
                json!({"print": {"command": "stop", "param": "", "sequence_id": next_studio_sequence_id()}})
            }
            Self::SetPrintSpeed(speed) => {
                json!({"print": {"command": "print_speed", "param": speed.as_u8().to_string(), "sequence_id": next_studio_sequence_id()}})
            }
            Self::GcodeLine(command) => {
                json!({"print": {"command": "gcode_line", "param": command.lines.join("\n"), "sequence_id": next_studio_sequence_id()}})
            }
            Self::AmsRereadRfid(command) => ams_reread_rfid_payload(command),
            Self::AmsLoadFilament(command) => ams_load_filament_payload(command),
            Self::AmsUnloadFilament(command) => ams_unload_filament_payload(command),
            Self::RawJson(payload) => payload.clone(),
            Self::ProjectFile(command) => project_file_payload(command),
        }
    }
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
    json!({"print": {"command": "ams_get_rfid", "sequence_id": next_studio_sequence_id(), "ams_id": command.ams_id, "slot_id": command.slot_id}})
}

fn ams_load_filament_payload(command: &AmsFilamentCommand) -> Value {
    let mut print = serde_json::Map::from_iter([
        ("command".to_owned(), json!("ams_change_filament")),
        ("sequence_id".to_owned(), json!(next_studio_sequence_id())),
        ("ams_id".to_owned(), json!(command.ams_id)),
        ("slot_id".to_owned(), json!(command.slot_id)),
        ("target".to_owned(), json!(command.target)),
        ("curr_temp".to_owned(), json!(-1)),
        ("tar_temp".to_owned(), json!(-1)),
    ]);
    if let Some(extruder_id) = command.extruder_id {
        print.insert("extruder_id".to_owned(), json!(extruder_id));
    }
    json!({ "print": print })
}

fn ams_unload_filament_payload(command: &AmsFilamentCommand) -> Value {
    let _ = command.slot_id;
    let _ = command.target;
    json!({"print": {"command": "ams_change_filament", "sequence_id": next_studio_sequence_id(), "ams_id": command.ams_id, "slot_id": 255, "target": 255, "curr_temp": 210, "tar_temp": 210}})
}

fn project_file_payload(command: &ProjectFileCommand) -> Value {
    let mut print = serde_json::Map::new();
    print.insert("command".to_owned(), json!("project_file"));
    print.insert("sequence_id".to_owned(), json!(next_studio_sequence_id()));
    print.insert(
        "param".to_owned(),
        json!(format!("Metadata/plate_{}.gcode", command.plate_id)),
    );
    print.insert(
        "url".to_owned(),
        json!(format!("ftp://{}", command.filename)),
    );
    print.insert("file".to_owned(), json!(command.filename));
    print.insert("task_id".to_owned(), json!(command.task_id));
    print.insert("subtask_id".to_owned(), json!(command.subtask_id));
    print.insert("use_ams".to_owned(), json!(command.use_ams));
    print.insert("flow_cali".to_owned(), json!(command.flow_cali));
    print.insert("timelapse".to_owned(), json!(command.timelapse));

    if let Some(mapping) = command
        .ams_mapping_json
        .as_deref()
        .and_then(project_file_ams_mapping)
    {
        print.insert("ams_mapping".to_owned(), mapping);
    }
    if let Some(mapping) = command
        .ams_mapping2_json
        .as_deref()
        .and_then(project_file_mapping_value)
    {
        print.insert("ams_mapping_2".to_owned(), mapping);
    }

    json!({ "print": print })
}

fn project_file_ams_mapping(raw: &str) -> Option<Value> {
    let Value::Array(values) = project_file_mapping_value(raw)? else {
        return None;
    };
    Some(Value::Array(
        values
            .into_iter()
            .map(|value| match value.as_i64() {
                Some(254 | 255) => json!(-1),
                _ => value,
            })
            .collect(),
    ))
}

fn project_file_mapping_value(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}
