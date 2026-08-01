use pandar_core::{BambuNozzleDevice, BambuNozzleInfo, StudioFiniteF64};
use serde::Serialize;

use super::super::{
    input::NozzleTemperature,
    scalar::{packed_temperature, text},
};

pub(super) fn fallback_nozzle_device(nozzles: &[NozzleTemperature]) -> BambuNozzleDevice {
    let total = nozzles.len().max(1);
    let mut exist = 0;
    let mut keyed_info = Vec::with_capacity(total);
    for index in 0..total {
        let label = nozzles
            .get(index)
            .map_or(String::new(), |nozzle| text(&nozzle.label));
        let id = studio_extruder_id(&label, index, total);
        exist |= 1_u32 << id;
        keyed_info.push((
            id,
            BambuNozzleInfo {
                id: id as i32,
                diameter: StudioFiniteF64::try_from(
                    (f64::from(studio_nozzle_diameter(nozzles.get(index))) * 10.0).round() / 10.0,
                )
                .expect("fallback nozzle diameter is finite"),
                nozzle_type: studio_nozzle_type(nozzles.get(index)),
                stat: Some(0),
                fila_id: None,
                wear: None,
                p_t: None,
                color_m: None,
            },
        ));
    }
    keyed_info.sort_by_key(|(id, _)| *id);
    BambuNozzleDevice {
        exist: Some(exist),
        state: Some(0),
        src_id: None,
        tar_id: None,
        info: keyed_info.into_iter().map(|(_, info)| info).collect(),
    }
}

#[derive(Serialize)]
pub(super) struct ExtruderDevice {
    state: usize,
    info: Vec<ExtruderInfo>,
}

impl ExtruderDevice {
    pub(super) fn new(
        nozzles: &[NozzleTemperature],
        active_nozzle: &str,
        physical_nozzle_routing: bool,
    ) -> Self {
        let total = nozzles.len().max(1);
        let active_id = studio_active_extruder_id(nozzles, active_nozzle);
        let mut keyed_info = Vec::with_capacity(total);
        for index in 0..total {
            let nozzle = nozzles.get(index);
            let label = nozzle.map_or(String::new(), |nozzle| text(&nozzle.label));
            let id = studio_extruder_id(&label, index, total);
            let temp = packed_temperature(
                &nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
                &nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
            );
            keyed_info.push((
                id,
                ExtruderInfo {
                    id,
                    filam_bak: Vec::new(),
                    info: 8,
                    temp,
                    spre: 65535,
                    snow: nozzle
                        .and_then(|nozzle| nozzle.snow)
                        .and_then(|value| u16::try_from(value).ok())
                        .unwrap_or(65535),
                    star: 65535,
                    stat: 0,
                    hnow: nozzle.and_then(|nozzle| nozzle.hnow).unwrap_or_else(|| {
                        if physical_nozzle_routing {
                            u32::from(u16::MAX)
                        } else {
                            id
                        }
                    }),
                },
            ));
        }
        keyed_info.sort_by_key(|(id, _)| *id);
        Self {
            state: total | ((active_id as usize) << 4),
            info: keyed_info.into_iter().map(|(_, info)| info).collect(),
        }
    }
}

#[derive(Serialize)]
struct ExtruderInfo {
    id: u32,
    filam_bak: Vec<u32>,
    info: u8,
    temp: u32,
    spre: u16,
    snow: u16,
    star: u16,
    stat: u8,
    hnow: u32,
}

fn studio_extruder_id(label: &str, index: usize, total: usize) -> u32 {
    if total <= 1 {
        return 0;
    }
    match label {
        label if label.eq_ignore_ascii_case("R") => 0,
        label if label.eq_ignore_ascii_case("L") => 1,
        _ => {
            if index == 0 {
                1
            } else {
                0
            }
        }
    }
}

fn studio_active_extruder_id(nozzles: &[NozzleTemperature], active_nozzle: &str) -> u32 {
    if nozzles.len() <= 1 {
        return 0;
    }
    if let Some(index) = nozzles
        .iter()
        .position(|nozzle| text(&nozzle.label).eq_ignore_ascii_case(active_nozzle))
    {
        return studio_extruder_id(&text(&nozzles[index].label), index, nozzles.len());
    }
    0
}

pub(super) fn studio_nozzle_by_id(
    nozzles: &[NozzleTemperature],
    id: u32,
) -> Option<&NozzleTemperature> {
    nozzles
        .iter()
        .enumerate()
        .find(|(index, nozzle)| {
            studio_extruder_id(&text(&nozzle.label), *index, nozzles.len()) == id
        })
        .map(|(_, nozzle)| nozzle)
}

pub(super) fn studio_nozzle_type(nozzle: Option<&NozzleTemperature>) -> String {
    let value = nozzle
        .map_or(String::new(), |nozzle| text(&nozzle.nozzle_type))
        .trim()
        .to_owned();
    if let Some(value) = studio_nozzle_code(&value) {
        return value;
    }
    match value.as_str() {
        "" => "XS01".to_owned(),
        "Hardened steel" | "Hardened Steel" => "hardened_steel".to_owned(),
        "Stainless steel" | "Stainless Steel" => "stainless_steel".to_owned(),
        "Tungsten carbide" | "Tungsten Carbide" => "tungsten_carbide".to_owned(),
        _ => value,
    }
}

fn studio_nozzle_code(value: &str) -> Option<String> {
    let mut chars = value.chars();
    let _prefix = chars.next()?;
    let flow = chars.next()?;
    let material: String = chars.collect();
    if material.len() != 2 {
        return None;
    }
    if !matches!(flow, 'S' | 'H' | 'U' | 'E' | 'A' | 'X') {
        return None;
    }
    if !matches!(material.as_str(), "00" | "01" | "05") {
        return None;
    }
    Some(format!("X{flow}{material}"))
}

pub(super) fn studio_nozzle_diameter(nozzle: Option<&NozzleTemperature>) -> f32 {
    let value = nozzle
        .map_or(String::new(), |nozzle| text(&nozzle.diameter_mm))
        .trim()
        .parse::<f32>()
        .unwrap_or(0.4);
    (value * 10.0).round() / 10.0
}
