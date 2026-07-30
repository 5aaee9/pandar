use super::normalized_string;
use crate::machine::mqtt::report::materials::*;

pub(super) fn derive_setting_id(filament_id: &str) -> String {
    let base = strip_version_suffix(filament_id);
    if let Some(rest) = base.strip_prefix("GFL") {
        return format!("GFSL{rest}");
    }
    base.to_owned()
}

pub(super) fn derive_filament_id(setting_id: &str) -> String {
    let base = strip_version_suffix(setting_id);
    if let Some(rest) = base.strip_prefix("GFSL") {
        return format!("GFL{rest}");
    }
    base.to_owned()
}

pub(super) fn strip_version_suffix(value: &str) -> &str {
    let Some((base, suffix)) = value.rsplit_once('_') else {
        return value;
    };
    if suffix.chars().all(|ch| ch.is_ascii_digit()) {
        base
    } else {
        value
    }
}

pub(super) fn unit_id(unit: &AmsUnitReport) -> Option<String> {
    normalized_string(unit.id.as_ref()).or_else(|| normalized_string(unit.ams_id.as_ref()))
}

pub(super) fn tray_id(tray: &MaterialSlotReport) -> Option<String> {
    normalized_string(tray.id.as_ref()).or_else(|| normalized_string(tray.tray_id.as_ref()))
}

pub(super) fn unit_kind(unit_id: &str) -> &'static str {
    match unit_id.parse::<u32>() {
        Ok(0..=63) => "ams",
        Ok(128..=135) => "ams_ht",
        _ => "unknown",
    }
}

pub(super) fn global_tray_id(unit_id: &str, tray_id: &str) -> Option<u64> {
    let unit_id = unit_id.parse::<u64>().ok()?;
    let tray_id = tray_id.parse::<u64>().ok()?;
    (unit_id < 64).then_some(unit_id * 4 + tray_id)
}

pub(super) fn parse_tray_exist_bits(value: Option<&ScalarValue>) -> Option<u64> {
    value?.parse_u64_or_hex()
}

pub(super) fn parse_i64(value: &ScalarValue) -> Option<i64> {
    value.parse_i64()
}
