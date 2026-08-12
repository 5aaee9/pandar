use std::collections::BTreeMap;

use super::{CreatePersonalPreset, PersonalPresetType};
use crate::repositories::{RepositoryError, RepositoryResult};

pub(super) const OWNER_PRESET_LIMIT: u64 = 1_000;
const MAX_MAP_BYTES: usize = 350 * 1024;
const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_NAME_BYTES: usize = 255;

pub(super) fn validate(input: &CreatePersonalPreset) -> RepositoryResult<()> {
    if input.name.trim().is_empty()
        || input.name.len() > MAX_NAME_BYTES
        || input.name.contains('\0')
    {
        return Err(RepositoryError::InvalidPersonalPreset);
    }
    if input.version.is_empty() || !valid_version(&input.version) {
        return Err(RepositoryError::InvalidPersonalPreset);
    }
    if !matches!(input.preset_type, PersonalPresetType::Filament) && input.filament_id.is_some() {
        return Err(RepositoryError::InvalidPersonalPreset);
    }
    validate_map(&input.options, envelope_bytes(input))
}

fn validate_map(options: &BTreeMap<String, String>, envelope_bytes: usize) -> RepositoryResult<()> {
    let mut bytes = envelope_bytes;
    for (key, value) in options {
        if key.is_empty()
            || key.len() > MAX_KEY_BYTES
            || key.contains('\0')
            || value.len() > MAX_VALUE_BYTES
        {
            return Err(RepositoryError::InvalidPersonalPreset);
        }
        bytes = bytes.saturating_add(key.len()).saturating_add(value.len());
    }
    if bytes > MAX_MAP_BYTES {
        return Err(RepositoryError::PersonalPresetTooLarge);
    }
    Ok(())
}

fn envelope_bytes(input: &CreatePersonalPreset) -> usize {
    [
        ("name", input.name.as_str()),
        ("type", input.preset_type.as_str()),
        ("version", input.version.as_str()),
        ("base_id", input.base_id.as_str()),
        ("inherits", input.inherits.as_deref().unwrap_or("")),
        ("filament_id", input.filament_id.as_deref().unwrap_or("")),
    ]
    .into_iter()
    .map(|(key, value)| key.len() + value.len())
    .sum()
}

fn valid_version(version: &str) -> bool {
    let mut count = 0;
    for part in version.split('.') {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        count += 1;
    }
    count >= 3
}
