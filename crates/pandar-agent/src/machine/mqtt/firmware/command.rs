use std::fmt;

use pandar_core::FirmwareCommand;
use serde::Serialize;

#[derive(Clone)]
pub(crate) struct FirmwareMqttCommand {
    command: String,
    sequence_id: String,
    response_domain: FirmwareResponseDomain,
    payload: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(crate) enum FirmwareResponseDomain {
    Info,
    Upgrade,
}

impl FirmwareMqttCommand {
    pub(crate) fn get_version(sequence_id: impl Into<String>) -> Self {
        let sequence_id = sequence_id.into();
        Self::new(
            "get_version",
            &sequence_id,
            FirmwareResponseDomain::Info,
            InfoEnvelope {
                info: GetVersionPayload {
                    command: "get_version",
                    sequence_id: &sequence_id,
                },
            },
        )
    }

    pub(crate) fn payload_bytes(&self) -> &[u8] {
        &self.payload
    }

    pub(crate) fn command(&self) -> &str {
        &self.command
    }

    pub(crate) fn sequence_id(&self) -> &str {
        &self.sequence_id
    }

    pub(crate) fn response_domain(&self) -> FirmwareResponseDomain {
        self.response_domain
    }

    fn new<T: Serialize>(
        command: impl Into<String>,
        sequence_id: &str,
        response_domain: FirmwareResponseDomain,
        payload: T,
    ) -> Self {
        Self {
            command: command.into(),
            sequence_id: sequence_id.to_owned(),
            response_domain,
            payload: serde_json::to_vec(&payload).expect("firmware MQTT payload is serializable"),
        }
    }
}

impl fmt::Debug for FirmwareMqttCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FirmwareMqttCommand")
            .field("command", &self.command)
            .field("sequence_id", &self.sequence_id)
            .field("payload", &"[redacted]")
            .finish()
    }
}

pub(crate) fn firmware_command_payload(command: &FirmwareCommand) -> FirmwareMqttCommand {
    match command {
        FirmwareCommand::UpgradeConfirm {
            sequence_id,
            src_id,
        } => FirmwareMqttCommand::new(
            "upgrade_confirm",
            sequence_id,
            FirmwareResponseDomain::Upgrade,
            UpgradeEnvelope {
                upgrade: UpgradeConfirmPayload {
                    command: "upgrade_confirm",
                    sequence_id,
                    src_id: *src_id,
                },
            },
        ),
        FirmwareCommand::ConsistencyConfirm {
            sequence_id,
            src_id,
        } => FirmwareMqttCommand::new(
            "consistency_confirm",
            sequence_id,
            FirmwareResponseDomain::Upgrade,
            UpgradeEnvelope {
                upgrade: UpgradeConfirmPayload {
                    command: "consistency_confirm",
                    sequence_id,
                    src_id: *src_id,
                },
            },
        ),
        FirmwareCommand::Start {
            sequence_id,
            src_id,
            url,
            module,
            version,
        } => FirmwareMqttCommand::new(
            "start",
            sequence_id,
            FirmwareResponseDomain::Upgrade,
            UpgradeEnvelope {
                upgrade: StartPayload {
                    command: "start",
                    sequence_id,
                    src_id: *src_id,
                    url,
                    module,
                    version,
                },
            },
        ),
        FirmwareCommand::SwitchAmsFirmware {
            sequence_id,
            src_id,
            id,
        } => FirmwareMqttCommand::new(
            "mc_for_ams_firmware_upgrade",
            sequence_id,
            FirmwareResponseDomain::Upgrade,
            UpgradeEnvelope {
                upgrade: SwitchAmsFirmwarePayload {
                    command: "mc_for_ams_firmware_upgrade",
                    sequence_id,
                    src_id: *src_id,
                    id: *id,
                },
            },
        ),
    }
}

#[derive(Serialize)]
struct InfoEnvelope<T> {
    info: T,
}

#[derive(Serialize)]
struct UpgradeEnvelope<T> {
    upgrade: T,
}

#[derive(Serialize)]
struct GetVersionPayload<'a> {
    command: &'static str,
    sequence_id: &'a str,
}

#[derive(Serialize)]
struct UpgradeConfirmPayload<'a> {
    command: &'static str,
    sequence_id: &'a str,
    src_id: i64,
}

#[derive(Serialize)]
struct StartPayload<'a> {
    command: &'static str,
    sequence_id: &'a str,
    src_id: i64,
    url: &'a str,
    module: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct SwitchAmsFirmwarePayload<'a> {
    command: &'static str,
    sequence_id: &'a str,
    src_id: i64,
    id: i32,
}
