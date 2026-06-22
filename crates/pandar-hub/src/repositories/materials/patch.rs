use anyhow::{Context, bail};
use serde_json::{Map, Value};
use time::OffsetDateTime;

#[derive(Debug)]
pub(super) struct ParsedPatch {
    pub(super) observed_at: String,
    pub(super) ams_units: Option<Vec<Value>>,
    pub(super) external_spools: Option<Vec<Value>>,
    pub(super) replace_external_spools: bool,
    pub(super) active_tray: Presence,
}

#[derive(Debug)]
pub(super) enum Presence {
    Absent,
    Null,
    Value(Value),
}

pub(super) fn parse_patch(raw: &str) -> Option<ParsedPatch> {
    if raw.trim().is_empty() {
        return None;
    }

    match parse_patch_result(raw).context("invalid material patch JSON") {
        Ok(patch) => Some(patch),
        Err(err) => {
            tracing::warn!(error = %sanitize_message(&format!("{err:#}")), "ignored material patch");
            None
        }
    }
}

pub(super) fn is_older(observed_at: &str, persisted_at: &str) -> anyhow::Result<bool> {
    Ok(
        parse_time(observed_at).context("failed to parse patch observed_at")?
            < parse_time(persisted_at).context("failed to parse persisted observed_at")?,
    )
}

pub(super) fn parse_array_json(raw: &str, context: &str) -> anyhow::Result<Vec<Value>> {
    serde_json::from_str::<Value>(raw)
        .with_context(|| format!("failed to parse {context}"))?
        .as_array()
        .cloned()
        .with_context(|| format!("{context} must be an array"))
}

pub(super) fn parse_object_json(raw: &str, context: &str) -> anyhow::Result<Value> {
    let value: Value =
        serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        bail!("{context} must be an object")
    }
}

fn parse_patch_result(raw: &str) -> anyhow::Result<ParsedPatch> {
    let value: Value = serde_json::from_str(raw).context("failed to parse material patch JSON")?;
    let object = value
        .as_object()
        .context("material patch root must be an object")?;
    if object.get("type").and_then(Value::as_str) != Some("printer_material_patch") {
        bail!("material patch type must be printer_material_patch");
    }
    let observed_at = object
        .get("observed_at")
        .and_then(Value::as_str)
        .context("material patch observed_at must be a string")?;
    parse_time(observed_at).context("material patch observed_at must be RFC3339 UTC")?;

    Ok(ParsedPatch {
        observed_at: observed_at.to_string(),
        ams_units: array_field(object, "ams_units")?,
        external_spools: array_field(object, "external_spools")?,
        replace_external_spools: object
            .get("replace_external_spools")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        active_tray: presence_field(object, "active_tray"),
    })
}

fn array_field(object: &Map<String, Value>, key: &str) -> anyhow::Result<Option<Vec<Value>>> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .with_context(|| format!("{key} must be an array"))?;
    Ok(Some(array.iter().map(filter_sensitive).collect()))
}

fn presence_field(object: &Map<String, Value>, key: &str) -> Presence {
    match object.get(key) {
        None => Presence::Absent,
        Some(Value::Null) => Presence::Null,
        Some(value) => Presence::Value(filter_sensitive(value)),
    }
}

fn filter_sensitive(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(filter_sensitive)
                .filter(|value| !value.is_null())
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, value)| !is_sensitive(key) && !scalar_is_sensitive(value))
                .map(|(key, value)| (key.clone(), filter_sensitive(value)))
                .collect(),
        ),
        value if scalar_is_sensitive(value) => Value::Null,
        value => value.clone(),
    }
}

fn scalar_is_sensitive(value: &Value) -> bool {
    value.as_str().map(is_sensitive).unwrap_or(false)
}

fn is_sensitive(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    ["access_code", "password", "passwd", "token", "auth"]
        .iter()
        .any(|needle| value.contains(needle))
}

fn sanitize_message(message: &str) -> String {
    ["access_code", "password", "passwd", "token", "auth"]
        .into_iter()
        .fold(message.to_string(), |message, needle| {
            message.replace(needle, "[redacted]")
        })
}

fn parse_time(value: &str) -> anyhow::Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .context("failed to parse timestamp")
}
