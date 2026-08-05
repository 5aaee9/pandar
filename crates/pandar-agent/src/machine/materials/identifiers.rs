use super::normalized_string;
use crate::machine::mqtt::report::materials::*;
use pandar_core::AmsUnitKind;

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

pub(super) fn unit_kind(
    unit_id: &str,
    info: Option<&ScalarValue>,
    tray_exist_bits: Option<u64>,
) -> Option<AmsUnitKind> {
    info.and_then(|value| normalized_string(Some(value)))
        .as_deref()
        .and_then(AmsUnitKind::from_studio_info)
        .or_else(|| {
            let fallback = AmsUnitKind::from_unit_id(unit_id);
            if fallback == AmsUnitKind::AmsHt {
                return Some(fallback);
            }
            let unit_id = unit_id.parse::<u64>().ok()?;
            let conventional_offset = u32::try_from(unit_id.checked_mul(4)?).ok()?;
            let conventional_mask = 0xF_u64.checked_shl(conventional_offset)?;
            tray_exist_bits
                .is_some_and(|bits| bits & conventional_mask != 0)
                .then_some(fallback)
        })
}

pub(super) fn global_tray_id(
    unit_id: &str,
    tray_id: &str,
    unit_kind: Option<AmsUnitKind>,
) -> Option<u64> {
    let unit_id = unit_id.parse::<u64>().ok()?;
    let tray_id = tray_id.parse::<u64>().ok()?;
    unit_kind?.global_tray_id(unit_id, tray_id)
}

pub(super) fn parse_tray_exist_bits(value: Option<&ScalarValue>) -> Option<u64> {
    value?.parse_u64_or_hex()
}

// Studio DevFilaSystem.cpp: dry_status lives in info bits 4..=7.
pub(super) fn parse_dry_status(value: Option<&ScalarValue>) -> Option<i64> {
    let raw = normalized_string(value)?;
    let parsed =
        u64::from_str_radix(raw.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    Some(((parsed >> 4) & 0xF) as i64)
}

pub(super) fn parse_i64(value: &ScalarValue) -> Option<i64> {
    value.parse_i64()
}
