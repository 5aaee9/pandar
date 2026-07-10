use std::collections::BTreeMap;

use anyhow::bail;
use serde::Serialize;
use serde_json::{Number, Value};

pub(super) mod payload;
mod print_error;
mod project_file;
mod sequence;

use payload::*;
pub use print_error::{HandlePrintErrorCommand, PrintErrorAction};
use project_file::project_file_payload;
use sequence::next_studio_sequence_id;
#[cfg(test)]
pub(crate) use sequence::next_studio_sequence_id_from;

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
    pub printer_model: Option<String>,
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
pub struct ChamberLightCommand {
    pub node: String,
    pub on: bool,
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

pub(crate) struct BambuMqttCommandPayload {
    pub payload: Value,
    pub sequence_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintReportProgress {
    pub serial: String,
    pub job_id: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub hms: Option<Vec<super::MachineHmsItem>>,
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
    pub payload: MachineReportDiagnosticPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MachineReportDiagnosticPayload {
    Object(BTreeMap<String, MachineReportDiagnosticPayload>),
    Array(Vec<MachineReportDiagnosticPayload>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BambuMqttCommand {
    GetVersion,
    RequestPushAll,
    PausePrint,
    ResumePrint,
    StopPrint,
    SetChamberLight(bool),
    SetChamberLightNode(ChamberLightCommand),
    SetPrintSpeed(PrintSpeed),
    SelectExtruder(u32),
    SetNozzleTemperature(SetNozzleTemperatureCommand),
    GcodeLine(GcodeLineCommand),
    AmsRereadRfid(AmsSlotCommand),
    AmsLoadFilament(AmsFilamentCommand),
    AmsUnloadFilament(AmsFilamentCommand),
    HandlePrintError(HandlePrintErrorCommand),
    RawJson(Value),
    ProjectFile(ProjectFileCommand),
}

impl BambuMqttCommand {
    pub fn payload(&self) -> Value {
        self.command_payload().payload
    }

    pub(crate) fn command_payload(&self) -> BambuMqttCommandPayload {
        match self {
            Self::GetVersion => info_payload("get_version"),
            Self::RequestPushAll => pushing_payload(),
            Self::PausePrint => basic_print_payload("pause"),
            Self::ResumePrint => basic_print_payload("resume"),
            Self::StopPrint => basic_print_payload("stop"),
            Self::SetChamberLight(on) => chamber_light_payload("chamber_light", *on),
            Self::SetChamberLightNode(command) => chamber_light_payload(&command.node, command.on),
            Self::SetPrintSpeed(speed) => print_speed_payload(*speed),
            Self::SelectExtruder(extruder_id) => select_extruder_payload(*extruder_id),
            Self::SetNozzleTemperature(command) => set_nozzle_temperature_payload(command),
            Self::GcodeLine(command) => gcode_line_payload(command),
            Self::AmsRereadRfid(command) => ams_reread_rfid_payload(command),
            Self::AmsLoadFilament(command) => ams_load_filament_payload(command),
            Self::AmsUnloadFilament(command) => ams_unload_filament_payload(command),
            Self::HandlePrintError(command) => print_error::print_error_payload(command),
            Self::RawJson(payload) => BambuMqttCommandPayload::without_sequence(payload.clone()),
            Self::ProjectFile(command) => project_file_payload(command),
        }
    }
}

impl BambuMqttCommandPayload {
    fn with_sequence(payload: Value, sequence_id: String) -> Self {
        Self {
            payload,
            sequence_id: Some(sequence_id),
        }
    }

    fn without_sequence(payload: Value) -> Self {
        Self {
            payload,
            sequence_id: None,
        }
    }
}

fn info_payload(command: &'static str) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(InfoPayload {
            info: InfoCommand {
                command,
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn pushing_payload() -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PushingPayload {
            pushing: PushingCommand {
                command: "pushall",
                sequence_id: sequence_id.clone(),
                version: 1,
                push_target: 1,
            },
        }),
        sequence_id,
    )
}

fn basic_print_payload(command: &'static str) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: BasicPrintCommand {
                command,
                param: "",
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn print_speed_payload(speed: PrintSpeed) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: PrintSpeedCommand {
                command: "print_speed",
                param: speed.as_u8().to_string(),
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn select_extruder_payload(extruder_id: u32) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: SelectExtruderCommand {
                command: "select_extruder",
                extruder_index: extruder_id,
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn set_nozzle_temperature_payload(
    command: &SetNozzleTemperatureCommand,
) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: SetNozzleTemperaturePayload {
                command: "set_nozzle_temp",
                extruder_index: command.extruder_id,
                target_temp: command.target_temp,
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn gcode_line_payload(command: &GcodeLineCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: GcodeLinePayload {
                command: "gcode_line",
                param: command.lines.join("\n"),
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn ams_reread_rfid_payload(command: &AmsSlotCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: AmsSlotPayload {
                command: "ams_get_rfid",
                sequence_id: sequence_id.clone(),
                ams_id: command.ams_id,
                slot_id: command.slot_id,
            },
        }),
        sequence_id,
    )
}

pub(crate) fn chamber_light_commands_for_nodes<'a>(
    nodes: impl IntoIterator<Item = &'a str>,
    on: bool,
) -> Vec<BambuMqttCommand> {
    nodes
        .into_iter()
        .map(|node| {
            BambuMqttCommand::SetChamberLightNode(ChamberLightCommand {
                node: node.to_owned(),
                on,
            })
        })
        .collect()
}

fn chamber_light_payload(node: &str, on: bool) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(SystemPayload {
            system: ChamberLightPayload {
                command: "ledctrl",
                led_node: node,
                led_mode: if on { "on" } else { "off" },
                led_on_time: 500,
                led_off_time: 500,
                loop_times: 1,
                interval_time: 1000,
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn ams_load_filament_payload(command: &AmsFilamentCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: AmsChangeFilamentPayload {
                command: "ams_change_filament",
                sequence_id: sequence_id.clone(),
                ams_id: command.ams_id,
                slot_id: command.slot_id,
                target: command.target,
                curr_temp: -1,
                tar_temp: -1,
                extruder_id: command.extruder_id,
            },
        }),
        sequence_id,
    )
}

fn ams_unload_filament_payload(command: &AmsFilamentCommand) -> BambuMqttCommandPayload {
    let _ = command.slot_id;
    let _ = command.target;
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: AmsChangeFilamentPayload {
                command: "ams_change_filament",
                sequence_id: sequence_id.clone(),
                ams_id: command.ams_id,
                slot_id: 255,
                target: 255,
                curr_temp: 210,
                tar_temp: 210,
                extruder_id: None,
            },
        }),
        sequence_id,
    )
}
