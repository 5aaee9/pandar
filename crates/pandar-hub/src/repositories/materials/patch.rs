use anyhow::{Context, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use time::OffsetDateTime;

#[derive(Debug)]
pub(super) struct ParsedPatch {
    pub(super) observed_at: String,
    pub(super) ams_units: Option<Vec<MaterialUnitPatch>>,
    pub(super) external_spools: Option<Vec<MaterialExternalSpoolPatch>>,
    pub(super) replace_external_spools: bool,
    pub(super) active_tray: Presence,
}

#[derive(Debug, Default)]
pub(super) enum Presence {
    #[default]
    Absent,
    Null,
    Value(Value),
}

#[derive(Debug, Deserialize)]
struct MaterialPatchDocument {
    #[serde(rename = "type")]
    kind: String,
    observed_at: String,
    #[serde(default, deserialize_with = "optional_filtered_array")]
    ams_units: Option<Vec<MaterialUnitPatch>>,
    #[serde(default, deserialize_with = "optional_filtered_array")]
    external_spools: Option<Vec<MaterialExternalSpoolPatch>>,
    #[serde(default)]
    replace_external_spools: bool,
    #[serde(default, deserialize_with = "presence")]
    active_tray: Presence,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct MaterialUnitPatch {
    #[serde(default)]
    pub(super) unit_id: Option<String>,
    #[serde(default)]
    pub(super) replace_trays: bool,
    #[serde(default, deserialize_with = "optional_filtered_array")]
    pub(super) trays: Option<Vec<MaterialTrayPatch>>,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct MaterialTrayPatch {
    #[serde(default)]
    pub(super) tray_id: Option<String>,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct MaterialExternalSpoolPatch {
    #[serde(default)]
    pub(super) external_id: Option<String>,
    #[serde(default)]
    pub(super) tray_id: Option<String>,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

impl MaterialUnitPatch {
    pub(super) fn fields_with_nulls(&self) -> Map<String, Value> {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls_for_new_unit(&self) -> Map<String, Value> {
        let mut object = self.fields.clone();
        if self.replace_trays {
            object.insert("replace_trays".to_owned(), Value::Bool(true));
        }
        object_without_null_fields(object)
    }
}

impl MaterialTrayPatch {
    pub(super) fn fields_with_nulls(&self) -> Map<String, Value> {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls(&self) -> Map<String, Value> {
        object_without_null_fields(self.fields.clone())
    }
}

impl MaterialExternalSpoolPatch {
    pub(super) fn key(&self) -> Option<(String, String)> {
        Some((self.external_id.clone()?, self.tray_id.clone()?))
    }

    pub(super) fn fields_with_nulls(&self) -> Map<String, Value> {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls(&self) -> Map<String, Value> {
        object_without_null_fields(self.fields.clone())
    }
}

pub(super) fn is_older(observed_at: &str, persisted_at: &str) -> anyhow::Result<bool> {
    Ok(
        parse_time(observed_at).context("failed to parse patch observed_at")?
            < parse_time(persisted_at).context("failed to parse persisted observed_at")?,
    )
}

pub(super) fn parse_array_json(raw: &str, context: &str) -> anyhow::Result<Vec<Value>> {
    serde_json::from_str::<Vec<Value>>(raw).with_context(|| format!("failed to parse {context}"))
}

pub(super) fn parse_object_json(raw: &str, context: &str) -> anyhow::Result<Value> {
    let object: Map<String, Value> =
        serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))?;
    Ok(Value::Object(object))
}

pub(super) fn parse_patch_result(raw: &str) -> anyhow::Result<ParsedPatch> {
    let document: MaterialPatchDocument =
        serde_json::from_str(raw).context("failed to parse material patch JSON")?;
    if document.kind != "printer_material_patch" {
        bail!("material patch type must be printer_material_patch");
    }
    parse_time(&document.observed_at).context("material patch observed_at must be RFC3339 UTC")?;

    Ok(ParsedPatch {
        observed_at: document.observed_at,
        ams_units: document.ams_units,
        external_spools: document.external_spools,
        replace_external_spools: document.replace_external_spools,
        active_tray: document.active_tray,
    })
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

pub(super) fn sanitize_message(message: &str) -> String {
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

fn optional_filtered_array<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    let Value::Array(values) = value else {
        return Err(serde::de::Error::custom("must be an array"));
    };
    values
        .iter()
        .map(filter_sensitive)
        .map(serde_json::from_value)
        .collect::<Result<Vec<T>, _>>()
        .map(Some)
        .map_err(serde::de::Error::custom)
}

fn presence<'de, D>(deserializer: D) -> Result<Presence, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Null => Presence::Null,
        value => Presence::Value(filter_sensitive(&value)),
    })
}

fn object_without_null_fields(object: Map<String, Value>) -> Map<String, Value> {
    object
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}
