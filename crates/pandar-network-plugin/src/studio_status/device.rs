use pandar_core::BambuNozzleDevice;
use serde::Serialize;

mod nozzle;

use nozzle::{
    ExtruderDevice, fallback_nozzle_device, studio_nozzle_by_id, studio_nozzle_diameter,
    studio_nozzle_type,
};

use super::{
    capabilities::{sdcard_available, studio_cfg, studio_fun, studio_fun2},
    input::PrinterStatus,
    materials::{AmsPayload, virtual_slots},
    scalar::{JsonNumber, json_number_or_zero, packed_temperature, text, text_if_present},
};

#[derive(Serialize)]
pub(super) struct StudioTelemetry {
    #[serde(skip_serializing_if = "Option::is_none")]
    cfg: Option<String>,
    fun: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fun2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aux: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gcode_state: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    printer_type: Option<String>,
    support_chamber: bool,
    support_chamber_temp_display: bool,
    sdcard: bool,
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
    ctt: JsonNumber,
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
        let chamber_target = json_number_or_zero(text(&printer.chamber_target_temperature_celsius));
        let supports_chamber = text_if_present(&printer.chamber_temperature_celsius).is_some()
            && text_if_present(&printer.chamber_target_temperature_celsius).is_some();
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
        let lights_report = printer
            .chamber_light_on
            .map(|on| LightReport {
                node: "chamber_light",
                mode: if on { "on" } else { "off" }.to_owned(),
            })
            .into_iter()
            .collect();

        Self {
            cfg: studio_cfg(
                printer
                    .materials
                    .as_ref()
                    .and_then(|materials| materials.cfg.as_deref()),
            ),
            fun: studio_fun(
                printer.fun.as_deref(),
                printer
                    .nozzle_system
                    .as_ref()
                    .is_some_and(|system| system.nozzle.info.iter().any(|nozzle| nozzle.id >= 16)),
            ),
            fun2: studio_fun2(printer.fun2.as_deref()),
            aux: printer
                .materials
                .as_ref()
                .and_then(|materials| materials.aux.clone()),
            stat: printer
                .materials
                .as_ref()
                .and_then(|materials| materials.stat.clone()),
            gcode_state: printer
                .gcode_state
                .as_deref()
                .or(printer.state.as_deref())
                .or(printer.task_status.as_deref())
                .map(str::trim)
                .filter(|state| !state.is_empty())
                .map(str::to_owned),
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
            printer_type: text_if_present(&printer.dev_model_name),
            support_chamber: supports_chamber,
            support_chamber_temp_display: supports_chamber,
            sdcard: sdcard_available(
                printer
                    .materials
                    .as_ref()
                    .and_then(|materials| materials.aux.as_deref()),
            ),
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
            ctt: JsonNumber::new(&chamber_target),
            lights_report,
            device: DeviceBlock::new(
                printer,
                &bed_current,
                &bed_target,
                &chamber_current,
                &chamber_target,
            ),
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
    connection_type: &'static str,
    bed_temp: u32,
    ctc: ChamberBlock,
    nozzle: BambuNozzleDevice,
    #[serde(skip_serializing_if = "Option::is_none")]
    holder: Option<pandar_core::BambuNozzleHolder>,
    extruder: ExtruderDevice,
}

impl DeviceBlock {
    fn new(
        printer: &PrinterStatus,
        bed_current: &str,
        bed_target: &str,
        chamber_current: &str,
        chamber_target: &str,
    ) -> Self {
        Self {
            kind: 1,
            connection_type: "cloud",
            bed_temp: packed_temperature(bed_current, bed_target),
            ctc: ChamberBlock {
                state: 1,
                info: ChamberInfo {
                    temp: packed_temperature(chamber_current, chamber_target),
                },
            },
            nozzle: printer
                .nozzle_system
                .as_ref()
                .map(|system| system.nozzle.clone())
                .unwrap_or_else(|| fallback_nozzle_device(&printer.nozzle_temperatures)),
            holder: printer
                .nozzle_system
                .as_ref()
                .and_then(|system| system.holder.clone()),
            extruder: ExtruderDevice::new(
                &printer.nozzle_temperatures,
                &text(&printer.active_nozzle),
                printer
                    .nozzle_system
                    .as_ref()
                    .is_some_and(|system| system.nozzle.info.iter().any(|nozzle| nozzle.id >= 16)),
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
