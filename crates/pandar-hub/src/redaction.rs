pub fn redact_secrets(message: &str) -> String {
    message
        .lines()
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn redact_link_printer_secret(message: &str, access_code: &str) -> String {
    let redacted = redact_secrets(message);
    if access_code.is_empty() {
        redacted
    } else {
        redacted.replace(access_code, "[redacted]")
    }
}

pub fn redact_link_printer_result_json(result_json: &str, access_code: &str) -> String {
    let redacted = redact_result_json(result_json);
    if access_code.is_empty() {
        return redact_link_printer_result_json_without_secret(&redacted);
    }

    match serde_json::from_str::<serde_json::Value>(&redacted) {
        Ok(mut value) => {
            if redact_json_string(&mut value, access_code) {
                value.to_string()
            } else {
                redacted
            }
        }
        Err(_) => redacted.replace(access_code, "[redacted]"),
    }
}

pub fn redact_link_printer_result_json_without_secret(result_json: &str) -> String {
    let redacted = redact_result_json(result_json);
    match serde_json::from_str::<serde_json::Value>(&redacted) {
        Ok(mut value) => {
            redact_all_json_strings(&mut value);
            value.to_string()
        }
        Err(_) => "[redacted]".to_owned(),
    }
}

pub fn redact_result_json(result_json: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(result_json) {
        Ok(mut value) => {
            if redact_json_value(&mut value) {
                value.to_string()
            } else {
                result_json.to_owned()
            }
        }
        Err(_) => redact_secrets(result_json),
    }
}

fn redact_json_value(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            let mut changed = false;
            for (key, value) in object {
                if is_credential_key(key) {
                    *value = serde_json::Value::String("[redacted]".to_owned());
                    changed = true;
                } else {
                    changed |= redact_json_value(value);
                }
            }
            changed
        }
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= redact_json_value(item);
            }
            changed
        }
        _ => false,
    }
}

fn redact_json_string(value: &mut serde_json::Value, secret: &str) -> bool {
    match value {
        serde_json::Value::String(value) if value.contains(secret) => {
            *value = value.replace(secret, "[redacted]");
            true
        }
        serde_json::Value::Number(number) => {
            let matches_secret = number.to_string() == secret;
            if matches_secret {
                *value = serde_json::Value::String("[redacted]".to_owned());
            }
            matches_secret
        }
        serde_json::Value::Object(object) => {
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
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= redact_json_string(item, secret);
            }
            changed
        }
        _ => false,
    }
}

fn redact_all_json_strings(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(value) => {
            *value = "[redacted]".to_owned();
            true
        }
        value @ serde_json::Value::Number(_) => {
            *value = serde_json::Value::String("[redacted]".to_owned());
            true
        }
        serde_json::Value::Object(object) => {
            let mut changed = false;
            let entries = std::mem::take(object);
            for (index, (_, mut value)) in entries.into_iter().enumerate() {
                changed = true;
                changed |= redact_all_json_strings(&mut value);
                object.insert(format!("[redacted_{index}]"), value);
            }
            changed
        }
        serde_json::Value::Array(items) => {
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

fn redact_line(line: &str) -> String {
    let mut redacted = line.to_owned();
    for key in [
        "authorization",
        "bearer",
        "ticket",
        "websocket_ticket",
        "plugin_ticket",
        "bambu_access_code",
        "access_code",
        "agent_credential",
        "credential",
        "artifact_path",
        "storage_path",
    ] {
        redacted = redact_key_value(&redacted, key);
    }

    let lower = redacted.to_ascii_lowercase();
    if lower.contains("artifact file ")
        || lower.contains("artifact directory ")
        || lower.contains("artifact spool ")
        || lower.contains("artifact storage path ")
    {
        return redact_after_marker(
            &redacted,
            &[" file ", " directory ", " spool ", " storage path "],
        );
    }
    if lower.starts_with("authorization:") {
        return "Authorization: [redacted]".to_owned();
    }
    if lower.contains("bearer ") {
        return redact_after_marker(&redacted, &["Bearer ", "bearer "]);
    }
    if lower.contains("agent credential ") {
        return redact_after_marker(&redacted, &["agent credential "]);
    }
    if lower.contains("plugin ticket ") {
        return redact_after_marker(&redacted, &["plugin ticket "]);
    }
    redacted
}

fn redact_after_marker(line: &str, markers: &[&str]) -> String {
    for marker in markers {
        if let Some((prefix, _)) = line.split_once(marker) {
            return format!("{prefix}{marker}[redacted]");
        }
    }
    line.to_owned()
}

fn redact_key_value(line: &str, key: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let Some(start) = lower.find(key) else {
        return line.to_owned();
    };
    let value_start = match line[start + key.len()..].chars().next() {
        Some('=') | Some(':') => start + key.len() + 1,
        Some('"') => {
            let rest = &line[start + key.len() + 1..];
            let Some(offset) = rest.find(':') else {
                return line.to_owned();
            };
            start + key.len() + 1 + offset + 1
        }
        _ => return line.to_owned(),
    };

    let value_start = line[value_start..]
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace() && *ch != '"')
        .map(|(offset, _)| value_start + offset)
        .unwrap_or(value_start);
    let value_end = line[value_start..]
        .char_indices()
        .find(|(_, ch)| matches!(ch, '"' | ',' | '&' | ' ' | '\t' | '\n'))
        .map(|(offset, _)| value_start + offset)
        .unwrap_or(line.len());

    format!("{}[redacted]{}", &line[..value_start], &line[value_end..])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        redact_link_printer_result_json, redact_link_printer_result_json_without_secret,
        redact_link_printer_secret, redact_secrets,
    };

    #[test]
    fn redacts_tokens_credentials_and_artifact_paths() {
        let message = "\
Authorization: Bearer tenant_secret_token
ticket=pandar_ws_abcdef
bambu_access_code=12345678
agent credential pandar_agent_abcdef
plugin ticket pandar_plugin_ticket_secret
plugin_ticket=pandar_plugin_ticket_query
{{\"agent_credential\":\"pandar_agent_json\",\"storage_path\":\"/tmp/pandar/spool/json.3mf\"}}
failed to remove artifact file /tmp/pandar/spool/tenant/artifact/plate.3mf";

        let redacted = redact_secrets(message);

        for forbidden in [
            "tenant_secret_token",
            "pandar_ws_abcdef",
            "12345678",
            "pandar_agent_abcdef",
            "pandar_plugin_ticket_secret",
            "pandar_plugin_ticket_query",
            "pandar_agent_json",
            "json.3mf",
            "/tmp/pandar",
            "plate.3mf",
        ] {
            assert!(
                !redacted.contains(forbidden),
                "{forbidden} was not redacted from {redacted}"
            );
        }
    }

    #[test]
    fn redacts_artifact_spool_paths_without_removing_cause_chain() {
        let message = "\
failed to create artifact spool /tmp/pandar/spool/not-a-directory

Caused by:
    Not a directory (os error 20)";

        let redacted = redact_secrets(message);

        assert!(redacted.contains("failed to create artifact spool [redacted]"));
        assert!(redacted.contains("Caused by:"));
        assert!(redacted.contains("Not a directory"));
        assert!(!redacted.contains("/tmp/pandar"));
        assert!(!redacted.contains("not-a-directory"));
    }

    #[test]
    fn redacts_link_printer_secret_as_key_value_and_standalone_value() {
        let message = "failed with access_code=SECRET-LINK-CODE\nCaused by:\n    printer rejected SECRET-LINK-CODE";

        let redacted = redact_link_printer_secret(message, "SECRET-LINK-CODE");

        assert!(redacted.contains("Caused by:"));
        assert!(!redacted.contains("SECRET-LINK-CODE"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn redacts_link_printer_secret_from_result_json_string_values() {
        let redacted = redact_link_printer_result_json(
            r#"{"message":"printer rejected SECRET-LINK-CODE"}"#,
            "SECRET-LINK-CODE",
        );

        assert!(!redacted.contains("SECRET-LINK-CODE"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn redacts_link_printer_result_json_strings_without_secret_context() {
        let redacted = redact_link_printer_result_json_without_secret(
            r#"{"message":"printer rejected SECRET-LINK-CODE","status":"failed"}"#,
        );

        assert!(!redacted.contains("SECRET-LINK-CODE"));
        let object: BTreeMap<String, String> = serde_json::from_str(&redacted).unwrap();
        assert!(object.keys().all(|key| key.starts_with("[redacted_")));
        assert!(object.values().all(|value| value == "[redacted]"));
    }
}
