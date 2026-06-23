use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;

use super::*;
use crate::Database;

mod auth_validation;
mod create;
mod read;
mod recovery;
mod redaction;

fn valid_request() -> serde_json::Value {
    json!({
        "filename": "plate.3mf",
        "content_type": "model/3mf",
        "artifact_base64": STANDARD.encode(b"abc"),
        "plate_id": 1,
        "use_ams": false,
        "flow_cali": false,
        "timelapse": false
    })
}
