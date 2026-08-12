use std::collections::BTreeMap;

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};

pub(super) const MAX_MAP_BYTES: usize = 350 * 1024;
const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_NAME_BYTES: usize = 255;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum PresetType {
    Print,
    Filament,
    Printer,
}

impl PresetType {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Filament => "filament",
            Self::Printer => "printer",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Metadata {
    pub(super) setting_id: String,
    #[serde(rename = "type")]
    pub(super) preset_type: PresetType,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) base_id: String,
    pub(super) inherits: Option<String>,
    pub(super) filament_id: Option<String>,
    pub(super) updated_time: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct ListResponse {
    pub(super) presets: Vec<Metadata>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FullPreset {
    pub(super) setting_id: String,
    #[serde(rename = "type")]
    pub(super) preset_type: PresetType,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) base_id: String,
    pub(super) inherits: Option<String>,
    pub(super) filament_id: Option<String>,
    pub(super) options: BTreeMap<String, String>,
    pub(super) updated_time: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct MutationResponse {
    pub(super) setting_id: String,
    pub(super) updated_time: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct ErrorResponse {
    pub(super) error: String,
    pub(super) code: Option<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct PresetRequest {
    #[serde(rename = "type")]
    pub(super) preset_type: PresetType,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) base_id: String,
    pub(super) inherits: Option<String>,
    pub(super) filament_id: Option<String>,
    pub(super) options: BTreeMap<String, String>,
}

impl PresetRequest {
    pub(super) fn from_flat(
        name: String,
        mut values: BTreeMap<String, String>,
    ) -> anyhow::Result<Self> {
        ensure!(
            !name.trim().is_empty() && name.len() <= MAX_NAME_BYTES && !name.contains('\0'),
            "invalid preset name"
        );
        let preset_type = match values.remove("type").as_deref() {
            Some("print") => PresetType::Print,
            Some("filament") => PresetType::Filament,
            Some("printer") => PresetType::Printer,
            _ => anyhow::bail!("invalid preset type"),
        };
        let version = values
            .remove("version")
            .context("preset version is missing")?;
        ensure!(valid_version(&version), "invalid preset version");
        let base_id = values.remove("base_id").unwrap_or_default();
        let inherits = values.remove("inherits").filter(|value| !value.is_empty());
        let filament_id = values
            .remove("filament_id")
            .filter(|value| !value.is_empty());
        ensure!(
            inherits
                .as_ref()
                .is_none_or(|value| value.len() <= MAX_VALUE_BYTES),
            "invalid inherits metadata"
        );
        ensure!(
            filament_id
                .as_ref()
                .is_none_or(|value| value.len() <= MAX_VALUE_BYTES),
            "invalid filament metadata"
        );
        for (key, value) in [
            ("type", preset_type.as_str()),
            ("version", version.as_str()),
            ("base_id", base_id.as_str()),
        ] {
            ensure!(
                key.len() <= MAX_KEY_BYTES && value.len() <= MAX_VALUE_BYTES,
                "invalid preset metadata"
            );
        }
        for key in ["setting_id", "user_id", "updated_time", "name"] {
            values.remove(key);
        }
        if preset_type != PresetType::Filament {
            ensure!(
                filament_id.is_none(),
                "filament id is only valid for filament presets"
            );
        }
        validate_map_size(&values)?;
        Ok(Self {
            preset_type,
            name,
            version,
            base_id,
            inherits,
            filament_id,
            options: values,
        })
    }
}

pub(super) fn metadata_map(value: &Metadata, user_id: &str) -> BTreeMap<String, String> {
    let mut map = envelope(
        &value.name,
        &value.setting_id,
        &value.preset_type,
        &value.version,
        &value.base_id,
        value.updated_time,
        user_id,
    );
    optional_metadata(
        &mut map,
        value.inherits.as_deref(),
        value.filament_id.as_deref(),
    );
    map
}

pub(super) fn full_map(value: FullPreset, user_id: &str) -> BTreeMap<String, String> {
    let mut map = value.options;
    for key in [
        "name",
        "type",
        "version",
        "setting_id",
        "base_id",
        "updated_time",
        "user_id",
        "inherits",
        "filament_id",
    ] {
        map.remove(key);
    }
    map.extend(envelope(
        &value.name,
        &value.setting_id,
        &value.preset_type,
        &value.version,
        &value.base_id,
        value.updated_time,
        user_id,
    ));
    optional_metadata(
        &mut map,
        value.inherits.as_deref(),
        value.filament_id.as_deref(),
    );
    map
}

fn envelope(
    name: &str,
    id: &str,
    kind: &PresetType,
    version: &str,
    base_id: &str,
    updated: i64,
    user_id: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("name".into(), name.into()),
        ("type".into(), kind.as_str().into()),
        ("version".into(), version.into()),
        ("setting_id".into(), id.into()),
        ("base_id".into(), base_id.into()),
        ("updated_time".into(), updated.to_string()),
        ("user_id".into(), user_id.into()),
    ])
}

fn optional_metadata(
    map: &mut BTreeMap<String, String>,
    inherits: Option<&str>,
    filament_id: Option<&str>,
) {
    if let Some(value) = inherits {
        map.insert("inherits".into(), value.into());
    }
    if let Some(value) = filament_id {
        map.insert("filament_id".into(), value.into());
    }
}

fn validate_map_size(values: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let mut total = 0usize;
    for (key, value) in values {
        ensure!(
            !key.is_empty() && key.len() <= MAX_KEY_BYTES && !key.contains('\0'),
            "invalid preset option key"
        );
        ensure!(
            value.len() <= MAX_VALUE_BYTES,
            "invalid preset option value"
        );
        total = total.saturating_add(key.len()).saturating_add(value.len());
    }
    ensure!(total <= MAX_MAP_BYTES, "personal preset is too large");
    Ok(())
}

fn valid_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() >= 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_map_admission_strips_server_fields_and_keeps_open_options() {
        let input = PresetRequest::from_flat(
            "Fine".into(),
            BTreeMap::from([
                ("type".into(), "print".into()),
                ("version".into(), "2.8.1.55".into()),
                ("setting_id".into(), "forged".into()),
                ("updated_time".into(), "7".into()),
                ("layer_height".into(), "0.16".into()),
            ]),
        )
        .unwrap();
        assert_eq!(
            input.options,
            BTreeMap::from([("layer_height".into(), "0.16".into())])
        );
    }

    #[test]
    fn metadata_and_full_maps_have_studio_required_envelope() {
        let metadata = Metadata {
            setting_id: "id".into(),
            preset_type: PresetType::Filament,
            name: "PLA".into(),
            version: "2.8.1.55".into(),
            base_id: String::new(),
            inherits: Some("Base".into()),
            filament_id: Some("P1".into()),
            updated_time: 42,
        };
        let map = metadata_map(&metadata, "user");
        assert_eq!(map["base_id"], "");
        assert_eq!(map["user_id"], "user");
        assert_eq!(map["updated_time"], "42");
    }

    #[test]
    fn admission_rejects_invalid_type_version_and_size() {
        for values in [
            BTreeMap::from([
                ("type".into(), "other".into()),
                ("version".into(), "2.8.1".into()),
            ]),
            BTreeMap::from([
                ("type".into(), "print".into()),
                ("version".into(), "bad".into()),
            ]),
            BTreeMap::from([
                ("type".into(), "print".into()),
                ("version".into(), "2.8.1".into()),
                ("x".into(), "z".repeat(MAX_VALUE_BYTES + 1)),
            ]),
        ] {
            assert!(PresetRequest::from_flat("Name".into(), values).is_err());
        }
    }
}
