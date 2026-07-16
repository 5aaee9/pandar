use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Number;
use time::OffsetDateTime;

#[derive(Debug)]
pub(super) struct ParsedPatch {
    pub(super) observed_at: String,
    pub(super) filament_switch_installed: Option<bool>,
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
    Value(MaterialJsonValue),
}

#[derive(Debug, Deserialize)]
struct MaterialPatchDocument {
    #[serde(rename = "type")]
    kind: String,
    observed_at: String,
    #[serde(default)]
    filament_switch_installed: Option<bool>,
    #[serde(default)]
    ams_units: Option<Vec<MaterialUnitPatch>>,
    #[serde(default)]
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
    #[serde(default)]
    pub(super) trays: Option<Vec<MaterialTrayPatch>>,
    #[serde(flatten)]
    fields: MaterialJsonObject,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct MaterialTrayPatch {
    #[serde(default)]
    pub(super) tray_id: Option<String>,
    #[serde(flatten)]
    fields: MaterialJsonObject,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct MaterialExternalSpoolPatch {
    #[serde(default)]
    pub(super) external_id: Option<String>,
    #[serde(default)]
    pub(super) tray_id: Option<String>,
    #[serde(flatten)]
    fields: MaterialJsonObject,
}

pub(super) type MaterialJsonObject = BTreeMap<String, MaterialJsonValue>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MaterialJsonValue {
    Object(MaterialJsonObject),
    Array(Vec<MaterialJsonValue>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl MaterialUnitPatch {
    pub(super) fn fields_with_nulls(&self) -> MaterialJsonObject {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls_for_new_unit(&self) -> MaterialJsonObject {
        let mut object = self.fields.clone();
        if self.replace_trays {
            object.insert("replace_trays".to_owned(), MaterialJsonValue::Bool(true));
        }
        object_without_null_fields(object)
    }
}

impl MaterialTrayPatch {
    pub(super) fn fields_with_nulls(&self) -> MaterialJsonObject {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls(&self) -> MaterialJsonObject {
        object_without_null_fields(self.fields.clone())
    }
}

impl MaterialExternalSpoolPatch {
    pub(super) fn key(&self) -> Option<(String, String)> {
        Some((self.external_id.clone()?, self.tray_id.clone()?))
    }

    pub(super) fn fields_with_nulls(&self) -> MaterialJsonObject {
        self.fields.clone()
    }

    pub(super) fn fields_without_nulls(&self) -> MaterialJsonObject {
        object_without_null_fields(self.fields.clone())
    }
}

pub(super) fn is_older(observed_at: &str, persisted_at: &str) -> anyhow::Result<bool> {
    Ok(
        parse_time(observed_at).context("failed to parse patch observed_at")?
            < parse_time(persisted_at).context("failed to parse persisted observed_at")?,
    )
}

pub(super) fn parse_array_json(raw: &str, context: &str) -> anyhow::Result<MaterialJsonValue> {
    let values: Vec<MaterialJsonValue> =
        serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))?;
    Ok(MaterialJsonValue::Array(values))
}

pub(super) fn parse_object_json(raw: &str, context: &str) -> anyhow::Result<MaterialJsonValue> {
    let object: MaterialJsonObject =
        serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))?;
    Ok(MaterialJsonValue::Object(object))
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
        filament_switch_installed: document.filament_switch_installed,
        ams_units: document.ams_units.map(redacted_units),
        external_spools: document.external_spools.map(redacted_external_spools),
        replace_external_spools: document.replace_external_spools,
        active_tray: document.active_tray.redacted(),
    })
}

fn is_sensitive(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    ["access_code", "password", "passwd", "token", "auth"]
        .iter()
        .any(|needle| value.contains(needle))
}

#[cfg(test)]
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

fn presence<'de, D>(deserializer: D) -> Result<Presence, D::Error>
where
    D: Deserializer<'de>,
{
    let value = MaterialJsonValue::deserialize(deserializer)?;
    Ok(match value {
        MaterialJsonValue::Null => Presence::Null,
        value => Presence::Value(value),
    })
}

fn object_without_null_fields(object: MaterialJsonObject) -> MaterialJsonObject {
    object
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn redacted_units(units: Vec<MaterialUnitPatch>) -> Vec<MaterialUnitPatch> {
    units
        .into_iter()
        .map(|unit| MaterialUnitPatch {
            fields: redact_object(unit.fields),
            trays: unit.trays.map(redacted_trays),
            ..unit
        })
        .collect()
}

fn redacted_trays(trays: Vec<MaterialTrayPatch>) -> Vec<MaterialTrayPatch> {
    trays
        .into_iter()
        .map(|tray| MaterialTrayPatch {
            fields: redact_object(tray.fields),
            ..tray
        })
        .collect()
}

fn redacted_external_spools(
    spools: Vec<MaterialExternalSpoolPatch>,
) -> Vec<MaterialExternalSpoolPatch> {
    spools
        .into_iter()
        .map(|spool| MaterialExternalSpoolPatch {
            fields: redact_object(spool.fields),
            ..spool
        })
        .collect()
}

fn redact_object(object: MaterialJsonObject) -> MaterialJsonObject {
    object
        .into_iter()
        .filter(|(key, value)| !is_sensitive(key) && !value.scalar_is_sensitive())
        .filter_map(|(key, value)| value.redacted().map(|value| (key, value)))
        .collect()
}

impl Presence {
    fn redacted(self) -> Self {
        match self {
            Self::Value(value) => value.redacted().map(Self::Value).unwrap_or(Self::Null),
            other => other,
        }
    }
}

impl MaterialJsonValue {
    fn redacted(self) -> Option<Self> {
        match self {
            Self::Object(object) => Some(Self::Object(redact_object(object))),
            Self::Array(values) => Some(Self::Array(
                values
                    .into_iter()
                    .filter_map(MaterialJsonValue::redacted)
                    .collect(),
            )),
            value if value.scalar_is_sensitive() => None,
            value => Some(value),
        }
    }

    pub(super) fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    fn scalar_is_sensitive(&self) -> bool {
        match self {
            Self::String(value) => is_sensitive(value),
            _ => false,
        }
    }
}
