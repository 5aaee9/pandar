use serde::{Deserialize, de::IgnoredAny};

const NOT_STATUS_REQUEST: i32 = 0;
const GET_VERSION_REQUEST: i32 = 1;
const PUSH_ALL_REQUEST: i32 = 2;

pub(crate) enum StudioStatusRequest {
    GetVersion { sequence_id: String },
    PushAll { sequence_id: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StudioStatusMessage {
    info: Option<StatusRequest>,
    pushing: Option<StatusRequest>,
}

#[derive(Deserialize)]
struct StatusRequest {
    command: String,
    #[serde(default)]
    sequence_id: StatusSequence,
}

#[derive(Default, Deserialize)]
#[serde(untagged)]
enum StatusSequence {
    String(String),
    Other(IgnoredAny),
    #[default]
    Absent,
}

pub(crate) fn parse_status_request(message: &str) -> Option<StudioStatusRequest> {
    let Ok(message) = serde_json::from_str::<StudioStatusMessage>(message) else {
        return None;
    };

    match (message.info, message.pushing) {
        (Some(info), None) if info.command == "get_version" => {
            Some(StudioStatusRequest::GetVersion {
                sequence_id: info.sequence_id.into_string(),
            })
        }
        (None, Some(pushing)) if pushing.command == "pushall" => {
            Some(StudioStatusRequest::PushAll {
                sequence_id: pushing.sequence_id.into_string(),
            })
        }
        _ => None,
    }
}

pub(super) fn classify_status_request(message: &str) -> (i32, String) {
    match parse_status_request(message) {
        Some(StudioStatusRequest::GetVersion { sequence_id }) => (GET_VERSION_REQUEST, sequence_id),
        Some(StudioStatusRequest::PushAll { sequence_id }) => (PUSH_ALL_REQUEST, sequence_id),
        None => (NOT_STATUS_REQUEST, String::new()),
    }
}

impl StatusSequence {
    fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Other(value) => {
                let _ = value;
                String::new()
            }
            Self::Absent => String::new(),
        }
    }
}
