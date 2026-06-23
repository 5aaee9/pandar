pub fn redact_secrets(message: &str) -> String {
    message
        .lines()
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
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
        || lower.contains("artifact storage path ")
    {
        return redact_after_marker(&redacted, &[" file ", " directory ", " storage path "]);
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
    use super::redact_secrets;

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
}
