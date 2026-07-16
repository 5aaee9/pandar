use serde::Serialize;

use super::{
    input::{AmsUnit, MaterialTray, Materials},
    scalar::{
        StudioScalar, hex_string, parse_u64_or_zero, scalar_if_present, text, text_if_present,
    },
};

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum AmsPayload {
    Empty {
        ams: Vec<AmsUnitPayload>,
    },
    WithBits {
        ams: Vec<AmsUnitPayload>,
        ams_exist_bits: String,
        tray_exist_bits: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tray_now: Option<String>,
    },
}

impl AmsPayload {
    pub(super) fn new(materials: Option<&Materials>) -> Self {
        let Some(materials) = materials else {
            return Self::Empty { ams: Vec::new() };
        };
        let mut ams_exist_bits = 0;
        let mut tray_exist_bits = 0;
        let ams = materials
            .ams_units
            .iter()
            .filter_map(|unit| {
                AmsUnitPayload::new(
                    unit,
                    &mut ams_exist_bits,
                    &mut tray_exist_bits,
                    materials.filament_switch_installed,
                )
            })
            .collect();
        Self::WithBits {
            ams,
            ams_exist_bits: hex_string(ams_exist_bits),
            tray_exist_bits: hex_string(tray_exist_bits),
            tray_now: tray_now(materials),
        }
    }
}

#[derive(Serialize)]
pub(super) struct AmsUnitPayload {
    id: String,
    info: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    humidity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    humidity_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temp: Option<String>,
    tray: Vec<StudioTray>,
}

impl AmsUnitPayload {
    fn new(
        unit: &AmsUnit,
        ams_exist_bits: &mut u64,
        tray_exist_bits: &mut u64,
        filament_switch_installed: Option<bool>,
    ) -> Option<Self> {
        let info = studio_ams_info(unit, filament_switch_installed)?;
        let unit_id = text_if_present(&unit.unit_id)?;
        let unit_number = parse_u64_or_zero(&unit_id);
        if unit_number < 64 {
            *ams_exist_bits |= 1_u64 << unit_number;
        }
        let tray = unit
            .trays
            .iter()
            .filter_map(|tray| {
                let global_number = global_tray_number(unit_number, tray);
                if global_number < 64 {
                    *tray_exist_bits |= 1_u64 << global_number;
                }
                StudioTray::from_material_tray(tray)
            })
            .collect();
        Some(Self {
            id: unit_id,
            info,
            humidity: text_if_present(&unit.humidity_level),
            humidity_raw: text_if_present(&unit.humidity),
            temp: text_if_present(&unit.temperature_celsius),
            tray,
        })
    }
}

fn studio_ams_info(unit: &AmsUnit, filament_switch_installed: Option<bool>) -> Option<String> {
    if filament_switch_installed == Some(true) {
        return filament_switch_info(unit);
    }

    let extruder_id = if text(&unit.toolhead).eq_ignore_ascii_case("L") {
        1
    } else {
        0
    };
    Some(hex_string(1 | (extruder_id << 8)))
}

fn filament_switch_info(unit: &AmsUnit) -> Option<String> {
    let info = text_if_present(&unit.info)?;
    let value = info
        .strip_prefix("0x")
        .or_else(|| info.strip_prefix("0X"))
        .unwrap_or(&info);
    let parsed = u64::from_str_radix(value, 16).ok()?;
    (((parsed >> 8) & 0xF == 0xE) && matches!((parsed >> 24) & 0xF, 0 | 1)).then_some(info)
}

#[derive(Serialize)]
pub(super) struct StudioTray {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_info_idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    k: Option<StudioScalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remain: Option<StudioScalar>,
}

impl StudioTray {
    fn from_material_tray(tray: &MaterialTray) -> Option<Self> {
        Some(Self {
            id: text_if_present(&tray.tray_id)?,
            tray_info_idx: text_if_present(&tray.filament_id),
            tray_type: text_if_present(&tray.filament_type),
            tray_color: text_if_present(&tray.color),
            k: scalar_if_present(&tray.k_value),
            remain: scalar_if_present(&tray.remaining_estimate),
        })
    }

    fn from_external_spool(spool: &MaterialTray, index: usize) -> Self {
        let toolhead = text(&spool.toolhead);
        let mut id = if toolhead.eq_ignore_ascii_case("L") {
            "254".to_string()
        } else if toolhead.eq_ignore_ascii_case("R") {
            "255".to_string()
        } else {
            text(&spool.external_id)
        };
        if id != "254" && id != "255" {
            id = if index == 0 { "255" } else { "254" }.to_string();
        }
        Self {
            id,
            tray_info_idx: text_if_present(&spool.filament_id),
            tray_type: text_if_present(&spool.filament_type),
            tray_color: text_if_present(&spool.color),
            k: scalar_if_present(&spool.k_value),
            remain: scalar_if_present(&spool.remaining_estimate),
        }
    }
}

pub(super) fn virtual_slots(materials: Option<&Materials>) -> Vec<StudioTray> {
    materials
        .map(|materials| {
            materials
                .external_spools
                .iter()
                .enumerate()
                .map(|(index, spool)| StudioTray::from_external_spool(spool, index))
                .collect()
        })
        .unwrap_or_default()
}

fn tray_now(materials: &Materials) -> Option<String> {
    let active = materials.active_tray.as_ref()?;
    text_if_present(&active.global_tray_id).or_else(|| {
        if text(&active.kind) == "external" {
            Some(text_if_present(&active.external_id).unwrap_or_else(|| "255".to_string()))
        } else {
            Some(
                (parse_u64_or_zero(&text(&active.ams_id)) * 4
                    + parse_u64_or_zero(&text(&active.tray_id)))
                .to_string(),
            )
        }
    })
}

fn global_tray_number(unit_number: u64, tray: &MaterialTray) -> u64 {
    text_if_present(&tray.global_tray_id)
        .map(|global| parse_u64_or_zero(&global))
        .unwrap_or_else(|| unit_number * 4 + parse_u64_or_zero(&text(&tray.tray_id)))
}
