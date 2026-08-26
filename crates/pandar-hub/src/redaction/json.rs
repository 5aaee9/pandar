//! Recursive JSON redaction over known-shape payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum RedactableJson {
    Object(BTreeMap<String, RedactableJson>),
    Array(Vec<RedactableJson>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl RedactableJson {
    pub(super) fn to_json_string(&self) -> String {
        serde_json::to_string(self).expect("redacted JSON is serializable")
    }
}

pub(super) fn redact_json_value(value: &mut RedactableJson) -> bool {
    match value {
        RedactableJson::Object(object) => {
            let mut changed = false;
            for (key, value) in object {
                if is_credential_key(key) {
                    *value = RedactableJson::String("[redacted]".to_owned());
                    changed = true;
                } else {
                    changed |= redact_json_value(value);
                }
            }
            changed
        }
        RedactableJson::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= redact_json_value(item);
            }
            changed
        }
        _ => false,
    }
}

pub(super) fn redact_json_string(value: &mut RedactableJson, secret: &str) -> bool {
    match value {
        RedactableJson::String(value) if value.contains(secret) => {
            *value = value.replace(secret, "[redacted]");
            true
        }
        RedactableJson::Number(number) => {
            let matches_secret = number.to_string() == secret;
            if matches_secret {
                *value = RedactableJson::String("[redacted]".to_owned());
            }
            matches_secret
        }
        RedactableJson::Object(object) => {
            let mut changed = false;
            let entries = std::mem::take(object);
            for (key, mut value) in entries {
                let redacted_key = if key.contains(secret) {
                    changed = true;
                    key.replace(secret, "[redacted]")
                } else {
                    key
                };
                changed |= redact_json_string(&mut value, secret);
                object.insert(redacted_key, value);
            }
            changed
        }
        RedactableJson::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= redact_json_string(item, secret);
            }
            changed
        }
        _ => false,
    }
}

pub(super) fn redact_all_json_strings(value: &mut RedactableJson) -> bool {
    match value {
        RedactableJson::String(value) => {
            *value = "[redacted]".to_owned();
            true
        }
        value @ RedactableJson::Number(_) => {
            *value = RedactableJson::String("[redacted]".to_owned());
            true
        }
        RedactableJson::Object(object) => {
            let mut changed = false;
            let entries = std::mem::take(object);
            for (index, (_, mut value)) in entries.into_iter().enumerate() {
                changed = true;
                changed |= redact_all_json_strings(&mut value);
                object.insert(format!("[redacted_{index}]"), value);
            }
            changed
        }
        RedactableJson::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= redact_all_json_strings(item);
            }
            changed
        }
        _ => false,
    }
}

fn is_credential_key(key: &str) -> bool {
    let normalized: String = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    [
        "accesscode",
        "password",
        "token",
        "auth",
        "credential",
        "ticket",
        "bearer",
    ]
    .iter()
    .any(|secret| normalized.contains(secret))
}
