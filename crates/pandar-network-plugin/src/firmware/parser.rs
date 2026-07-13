use std::fmt;

use serde::{Deserialize, de::Visitor};

use super::model::{StudioFirmwareCommand, StudioFirmwareParse};

pub const PLUGIN_JSON_BODY_LIMIT: usize = 64 * 1024;

#[derive(Default, Deserialize)]
struct StudioEnvelope {
    #[serde(default, deserialize_with = "present_upgrade")]
    upgrade: UpgradePresence,
}

#[derive(Default)]
enum UpgradePresence {
    #[default]
    Absent,
    Present(StudioFirmwareCommand),
}

#[derive(Deserialize)]
#[serde(tag = "command")]
enum StudioFirmwareWire {
    #[serde(rename = "upgrade_confirm")]
    UpgradeConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "consistency_confirm")]
    ConsistencyConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "start")]
    Start {
        sequence_id: String,
        src_id: i64,
        url: String,
        module: String,
        version: String,
    },
    #[serde(rename = "mc_for_ams_firmware_upgrade")]
    McForAmsFirmwareUpgrade {
        sequence_id: String,
        src_id: i64,
        id: i32,
    },
}

pub fn parse_studio_firmware(message: &str) -> StudioFirmwareParse {
    if message.len() > PLUGIN_JSON_BODY_LIMIT {
        return if upgrade_key_is_present(message) {
            StudioFirmwareParse::InvalidFirmware
        } else {
            StudioFirmwareParse::NotFirmware
        };
    }
    match serde_json::from_str::<StudioEnvelope>(message) {
        Ok(StudioEnvelope {
            upgrade: UpgradePresence::Absent,
        }) => StudioFirmwareParse::NotFirmware,
        Ok(StudioEnvelope {
            upgrade: UpgradePresence::Present(command),
        }) if valid_command(&command) => StudioFirmwareParse::Firmware(command),
        Ok(_) => StudioFirmwareParse::InvalidFirmware,
        Err(_) if upgrade_key_is_present(message) => StudioFirmwareParse::InvalidFirmware,
        Err(_) => StudioFirmwareParse::NotFirmware,
    }
}

fn present_upgrade<'de, D>(deserializer: D) -> Result<UpgradePresence, D::Error>
where
    D: serde::Deserializer<'de>,
{
    StudioFirmwareWire::deserialize(deserializer)
        .map(StudioFirmwareCommand::from)
        .map(UpgradePresence::Present)
}

fn valid_command(command: &StudioFirmwareCommand) -> bool {
    match command {
        StudioFirmwareCommand::Start {
            url,
            module,
            version,
            ..
        } => !url.is_empty() && !module.is_empty() && !version.is_empty(),
        _ => true,
    }
}

fn upgrade_key_is_present(message: &str) -> bool {
    struct UpgradeKeyVisitor;

    impl<'de> Visitor<'de> for UpgradeKeyVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a Studio message object")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut present = false;
            while let Some(key) = map.next_key::<String>()? {
                let _: serde::de::IgnoredAny = map.next_value()?;
                present |= key == "upgrade";
            }
            Ok(present)
        }
    }

    let mut deserializer = serde_json::Deserializer::from_str(message);
    let present = serde::de::Deserializer::deserialize_map(&mut deserializer, UpgradeKeyVisitor);
    present.is_ok_and(|present| present && deserializer.end().is_ok())
}

impl From<StudioFirmwareWire> for StudioFirmwareCommand {
    fn from(command: StudioFirmwareWire) -> Self {
        match command {
            StudioFirmwareWire::UpgradeConfirm {
                sequence_id,
                src_id,
            } => Self::UpgradeConfirm {
                sequence_id,
                src_id,
            },
            StudioFirmwareWire::ConsistencyConfirm {
                sequence_id,
                src_id,
            } => Self::ConsistencyConfirm {
                sequence_id,
                src_id,
            },
            StudioFirmwareWire::Start {
                sequence_id,
                src_id,
                url,
                module,
                version,
            } => Self::Start {
                sequence_id,
                src_id,
                url,
                module,
                version,
            },
            StudioFirmwareWire::McForAmsFirmwareUpgrade {
                sequence_id,
                src_id,
                id,
            } => Self::McForAmsFirmwareUpgrade {
                sequence_id,
                src_id,
                id,
            },
        }
    }
}
