use super::{
    BambuMqttCommandPayload,
    payload::{
        AmsChangeFilamentPayload, AmsFilamentDryingPayload, AmsSlotPayload, PrintPayload,
        json_payload,
    },
    sequence::next_studio_sequence_id,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmsDryingCommand {
    pub ams_id: u32,
    pub temperature_celsius: u16,
    pub duration_hours: u16,
    pub filament: String,
    pub rotate_tray: bool,
}

pub(super) fn ams_reread_rfid_payload(command: &AmsSlotCommand) -> BambuMqttCommandPayload {
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

pub(super) fn ams_load_filament_payload(command: &AmsFilamentCommand) -> BambuMqttCommandPayload {
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

pub(super) fn ams_unload_filament_payload(command: &AmsFilamentCommand) -> BambuMqttCommandPayload {
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

pub(super) fn ams_start_drying_payload(command: &AmsDryingCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: AmsFilamentDryingPayload {
                command: "ams_filament_drying",
                sequence_id: sequence_id.clone(),
                ams_id: command.ams_id,
                mode: 1,
                filament: &command.filament,
                temp: command.temperature_celsius,
                duration: command.duration_hours,
                humidity: 0,
                rotate_tray: command.rotate_tray,
                cooling_temp: 20,
                close_power_conflict: false,
            },
        }),
        sequence_id,
    )
}

pub(super) fn ams_stop_drying_payload(ams_id: u32) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: AmsFilamentDryingPayload {
                command: "ams_filament_drying",
                sequence_id: sequence_id.clone(),
                ams_id,
                mode: 0,
                filament: "",
                temp: 0,
                duration: 0,
                humidity: 0,
                rotate_tray: false,
                cooling_temp: 0,
                close_power_conflict: false,
            },
        }),
        sequence_id,
    )
}
