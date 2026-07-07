use serde::Serialize;

use super::{
    input::{NozzleTemperature, PrinterStatus},
    materials::{AmsPayload, virtual_slots},
    scalar::{JsonNumber, json_number_or_zero, packed_temperature, text, text_if_present},
};

#[derive(Serialize)]
pub(super) struct StudioTelemetry {
    printer_type: String,
    support_chamber: bool,
    support_chamber_temp_display: bool,
    bed_temper: JsonNumber,
    bed_target_temper: JsonNumber,
    nozzle_type: &'static str,
    nozzle_diameter: f32,
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
        let nozzle = printer.nozzle_temperatures.first();
        let right_nozzle = printer.nozzle_temperatures.get(1);
        let bed_current = json_number_or_zero(text(&printer.bed_temperature_celsius));
        let bed_target = json_number_or_zero(text(&printer.bed_target_temperature_celsius));
        let chamber_current = json_number_or_zero(text(&printer.chamber_temperature_celsius));
        let nozzle_current = json_number_or_zero(
            nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
        );
        let nozzle_target = json_number_or_zero(
            nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
        );
        let right_nozzle_current = json_number_or_zero(
            right_nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
        );
        let right_nozzle_target = json_number_or_zero(
            right_nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
        );
        let light_mode = if printer.chamber_light_on {
            "on"
        } else {
            "off"
        };

        Self {
            printer_type: text_if_present(&printer.dev_model_name)
                .unwrap_or_else(|| "C11".to_string()),
            support_chamber: true,
            support_chamber_temp_display: true,
            bed_temper: JsonNumber::new(&bed_current),
            bed_target_temper: JsonNumber::new(&bed_target),
            nozzle_type: "XS01",
            nozzle_diameter: 0.4,
            nozzle_temper: JsonNumber::new(&nozzle_current),
            nozzle_target_temper: JsonNumber::new(&nozzle_target),
            nozzle_temper2: JsonNumber::new(&right_nozzle_current),
            nozzle_target_temper2: JsonNumber::new(&right_nozzle_target),
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
        let mut info = Vec::with_capacity(total);
        for index in 0..total {
            let label = nozzles
                .get(index)
                .map_or(String::new(), |nozzle| text(&nozzle.label));
            let id = studio_extruder_id(&label, index, total);
            exist |= 1_u32 << id;
            info.push(NozzleInfo {
                id,
                diameter: 0.4,
                nozzle_type: "XS01",
                stat: 0,
            });
        }
        Self {
            exist,
            state: 0,
            info,
        }
    }
}

#[derive(Serialize)]
struct NozzleInfo {
    id: u32,
    diameter: f32,
    #[serde(rename = "type")]
    nozzle_type: &'static str,
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
        let mut info = Vec::with_capacity(total);
        for index in 0..total {
            let nozzle = nozzles.get(index);
            let label = nozzle.map_or(String::new(), |nozzle| text(&nozzle.label));
            let id = studio_extruder_id(&label, index, total);
            let temp = packed_temperature(
                &nozzle.map_or(String::new(), |nozzle| text(&nozzle.current_celsius)),
                &nozzle.map_or(String::new(), |nozzle| text(&nozzle.target_celsius)),
            );
            info.push(ExtruderInfo {
                id,
                info: 8,
                temp,
                spre: 65535,
                snow: 65535,
                star: 65535,
                stat: 0,
                hnow: id,
            });
        }
        Self {
            state: total | ((active_id as usize) << 4),
            info,
        }
    }
}

#[derive(Serialize)]
struct ExtruderInfo {
    id: u32,
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
    if label.eq_ignore_ascii_case("L") {
        return 1;
    }
    if label.eq_ignore_ascii_case("R") {
        return 0;
    }
    if index == 0 { 1 } else { 0 }
}

fn studio_active_extruder_id(nozzles: &[NozzleTemperature], active_nozzle: &str) -> u32 {
    if nozzles.len() <= 1 {
        return 0;
    }
    if active_nozzle.eq_ignore_ascii_case("L") {
        return 1;
    }
    if active_nozzle.eq_ignore_ascii_case("R") {
        return 0;
    }
    studio_extruder_id(&text(&nozzles[0].label), 0, nozzles.len())
}
