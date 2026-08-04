use serde::Serialize;

use super::{
    BambuMqttCommandPayload,
    payload::{PrintPayload, json_payload},
    sequence::next_studio_sequence_id,
};

#[derive(Serialize)]
struct NozzleHolderCtrlPayload {
    command: &'static str,
    sequence_id: String,
    action: u32,
}

#[derive(Serialize)]
struct RackNozzlePayload {
    command: &'static str,
    sequence_id: String,
    id: u32,
}

pub(super) fn nozzle_holder_ctrl_payload(action: u32) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: NozzleHolderCtrlPayload {
                command: "nozzle_holder_ctrl",
                sequence_id: sequence_id.clone(),
                action,
            },
        }),
        sequence_id,
    )
}

pub(super) fn rack_nozzle_payload(command: &'static str, id: u32) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(PrintPayload {
            print: RackNozzlePayload {
                command,
                sequence_id: sequence_id.clone(),
                id,
            },
        }),
        sequence_id,
    )
}
