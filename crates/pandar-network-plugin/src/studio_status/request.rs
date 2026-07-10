use serde::{Deserialize, de::IgnoredAny};

const NOT_STATUS_REQUEST: i32 = 0;
const GET_VERSION_REQUEST: i32 = 1;
const PUSH_ALL_REQUEST: i32 = 2;

#[derive(Deserialize)]
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

pub(super) fn classify_status_request(message: &str) -> (i32, String) {
    let Ok(message) = serde_json::from_str::<StudioStatusMessage>(message) else {
        return (NOT_STATUS_REQUEST, String::new());
    };

    if let Some(info) = message.info
        && info.command == "get_version"
    {
        return (GET_VERSION_REQUEST, info.sequence_id.into_string());
    }
    if let Some(pushing) = message.pushing
        && pushing.command == "pushall"
    {
        return (PUSH_ALL_REQUEST, pushing.sequence_id.into_string());
    }
    (NOT_STATUS_REQUEST, String::new())
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
