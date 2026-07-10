use serde::Serialize;

use super::{
    input::{NozzleTemperature, PrinterStatus},
    materials::{AmsPayload, virtual_slots},
    scalar::{JsonNumber, json_number_or_zero, packed_temperature, text, text_if_present},
};

#[derive(Serialize)]
pub(super) struct StudioTelemetry {
    gcode_state: String,
    mc_percent: u8,
    mc_remaining_time: u32,
    layer_num: u32,
    total_layer_num: u32,
    task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    print_error: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    project_id: &'static str,
    profile_id: &'static str,
    subtask_id: String,
    gcode_file: String,
    subtask_name: String,
    hms: Vec<super::input::PrinterHms>,
    printer_type: String,
    support_chamber: bool,
    support_chamber_temp_display: bool,
    bed_temper: JsonNumber,
    bed_target_temper: JsonNumber,
    nozzle_type: String,
    nozzle_diameter: f32,
    nozzle_type2: String,
    nozzle_diameter2: f32,
    nozzle_temper: JsonNumber,
    nozzle_target_temper: JsonNumber,
    nozzle_temper2: JsonNumber,
    nozzle_target_temper2: JsonNumber,
    chamber_temper: JsonNumber,
    lights_report: Vec<LightReport>,
    device: DeviceBlock,
    ams: AmsPayload,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    vir_slot: Vec<super::materials::StudioTray>,
}

impl From<&PrinterStatus> for StudioTelemetry {
    fn from(printer: &PrinterStatus) -> Self {
        let main_nozzle = studio_nozzle_by_id(&printer.nozzle_temperatures, 0);
        let auxiliary_nozzle = studio_nozzle_by_id(&printer.nozzle_temperatures, 1);
        let bed_current = json_number_or_zero(text(&printer.bed_temperature_celsius));
        let bed_target = json_number_or_zero(text(&printer.bed_target_temperature_celsius));
        let chamber_current = json_number_or_zero(text(&printer.chamber_temperature_celsius));
        let nozzle_current = json_number_or_zero(
            main_nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
        );
        let nozzle_target = json_number_or_zero(
            main_nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
        );
        let auxiliary_nozzle_current = json_number_or_zero(
            auxiliary_nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
        );
        let auxiliary_nozzle_target = json_number_or_zero(
            auxiliary_nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
        );
        let light_mode = if printer.chamber_light_on.unwrap_or_default() {
            "on"
        } else {
            "off"
        };

        Self {
            gcode_state: printer
                .gcode_state
                .clone()
                .unwrap_or_else(|| "IDLE".to_owned()),
            mc_percent: printer.mc_percent.unwrap_or_default(),
            mc_remaining_time: printer.mc_remaining_time.unwrap_or_default(),
            layer_num: printer.layer_num.unwrap_or_default(),
            total_layer_num: printer.total_layer_num.unwrap_or_default(),
            task_id: printer.task_id.clone().unwrap_or_else(|| "0".to_owned()),
            print_error: printer.print_error,
            job_id: printer.job_id.clone(),
            project_id: "0",
            profile_id: "0",
            subtask_id: printer.subtask_id.clone().unwrap_or_else(|| "0".to_owned()),
            gcode_file: printer.gcode_file.clone().unwrap_or_default(),
            subtask_name: printer.subtask_name.clone().unwrap_or_default(),
            hms: printer.hms.clone(),
            printer_type: text_if_present(&printer.dev_model_name)
                .unwrap_or_else(|| "C11".to_string()),
            support_chamber: true,
            support_chamber_temp_display: true,
            bed_temper: JsonNumber::new(&bed_current),
            bed_target_temper: JsonNumber::new(&bed_target),
            nozzle_type: studio_nozzle_type(main_nozzle),
            nozzle_diameter: studio_nozzle_diameter(main_nozzle),
            nozzle_type2: studio_nozzle_type(auxiliary_nozzle),
            nozzle_diameter2: studio_nozzle_diameter(auxiliary_nozzle),
            nozzle_temper: JsonNumber::new(&nozzle_current),
            nozzle_target_temper: JsonNumber::new(&nozzle_target),
            nozzle_temper2: JsonNumber::new(&auxiliary_nozzle_current),
            nozzle_target_temper2: JsonNumber::new(&auxiliary_nozzle_target),
            chamber_temper: JsonNumber::new(&chamber_current),
            lights_report: vec![LightReport {
                node: "chamber_light",
                mode: light_mode.to_string(),
            }],
            device: DeviceBlock::new(printer, &bed_current, &bed_target, &chamber_current),
            ams: AmsPayload::new(printer.materials.as_ref()),
            vir_slot: virtual_slots(printer.materials.as_ref()),
        }
    }
}

#[derive(Serialize)]
struct LightReport {
    node: &'static str,
    mode: String,
}

#[derive(Serialize)]
struct DeviceBlock {
    #[serde(rename = "type")]
    kind: u8,
    bed_temp: u32,
    ctc: ChamberBlock,
    nozzle: NozzleDevice,
    extruder: ExtruderDevice,
}

impl DeviceBlock {
    fn new(
        printer: &PrinterStatus,
        bed_current: &str,
        bed_target: &str,
        chamber_current: &str,
    ) -> Self {
        Self {
            kind: 1,
            bed_temp: packed_temperature(bed_current, bed_target),
            ctc: ChamberBlock {
                state: 1,
                info: ChamberInfo {
                    temp: packed_temperature(chamber_current, ""),
                },
            },
            nozzle: NozzleDevice::new(&printer.nozzle_temperatures),
            extruder: ExtruderDevice::new(
                &printer.nozzle_temperatures,
                &text(&printer.active_nozzle),
            ),
        }
    }
}

#[derive(Serialize)]
struct ChamberBlock {
    state: u8,
    info: ChamberInfo,
}

#[derive(Serialize)]
struct ChamberInfo {
    temp: u32,
}

#[derive(Serialize)]
struct NozzleDevice {
    exist: u32,
    state: u8,
    info: Vec<NozzleInfo>,
}

impl NozzleDevice {
    fn new(nozzles: &[NozzleTemperature]) -> Self {
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
                NozzleInfo {
                    id,
                    diameter: studio_nozzle_diameter(nozzles.get(index)),
                    nozzle_type: studio_nozzle_type(nozzles.get(index)),
                    stat: 0,
                },
            ));
        }
        keyed_info.sort_by_key(|(id, _)| *id);
        Self {
            exist,
            state: 0,
            info: keyed_info.into_iter().map(|(_, info)| info).collect(),
        }
    }
}

#[derive(Serialize)]
struct NozzleInfo {
    id: u32,
    diameter: f32,
    #[serde(rename = "type")]
    nozzle_type: String,
    stat: u8,
}

#[derive(Serialize)]
struct ExtruderDevice {
    state: usize,
    info: Vec<ExtruderInfo>,
}

impl ExtruderDevice {
    fn new(nozzles: &[NozzleTemperature], active_nozzle: &str) -> Self {
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
                    snow: 65535,
                    star: 65535,
                    stat: 0,
                    hnow: id,
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

fn studio_nozzle_by_id(nozzles: &[NozzleTemperature], id: u32) -> Option<&NozzleTemperature> {
    nozzles
        .iter()
        .enumerate()
        .find(|(index, nozzle)| {
            studio_extruder_id(&text(&nozzle.label), *index, nozzles.len()) == id
        })
        .map(|(_, nozzle)| nozzle)
}

fn studio_nozzle_type(nozzle: Option<&NozzleTemperature>) -> String {
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

fn studio_nozzle_diameter(nozzle: Option<&NozzleTemperature>) -> f32 {
    let value = nozzle
        .map_or(String::new(), |nozzle| text(&nozzle.diameter_mm))
        .trim()
        .parse::<f32>()
        .unwrap_or(0.4);
    (value * 10.0).round() / 10.0
}
