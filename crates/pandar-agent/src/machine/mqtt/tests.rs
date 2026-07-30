use std::{sync::atomic::AtomicU32, time::Duration};

use pandar_core::PrintCalibrationMode;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::*;
use crate::machine::BambuPrinterEndpoint;
use crate::{
    AgentConfig,
    protocol::agent::v1::{PrintJobReport, PrintSubmissionSource, agent_event},
};

mod device_features;
mod firmware_ambiguity;
mod firmware_control;
mod firmware_domain;
mod firmware_observation;
mod firmware_persistent;
mod firmware_session;
mod fixtures;
mod hms;
mod print_error;
mod recovery;
mod snapshot;
mod tls;

use fixtures::*;

mod commands;
mod project_file;
mod refresh;
mod reports;
mod studio_print;

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: "01S00EXAMPLE".to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some("garage-a1".to_string()),
    }
}

fn print_report_from_json(
    endpoint: &BambuPrinterEndpoint,
    report: &serde_json::Value,
) -> PrintReportProgress {
    print_report_from_report(endpoint, &MachineReport::decode(report.clone()))
}

fn get_version_report(model: &str) -> serde_json::Value {
    serde_json::to_value(TestGetVersionReport {
        info: TestGetVersionInfo {
            command: "get_version",
            module: vec![
                TestGetVersionModule {
                    name: "wifi",
                    sw_ver: None,
                    product_name: "ignored",
                    sn: None,
                },
                TestGetVersionModule {
                    name: "ota",
                    sw_ver: Some("01.08.01.00"),
                    product_name: model,
                    sn: Some("01S00EXAMPLE"),
                },
            ],
        },
    })
    .unwrap()
}

#[derive(Debug, Serialize)]
struct TestGetVersionReport<'a> {
    info: TestGetVersionInfo<'a>,
}

#[derive(Debug, Serialize)]
struct TestGetVersionInfo<'a> {
    command: &'static str,
    module: Vec<TestGetVersionModule<'a>>,
}

#[derive(Debug, Serialize)]
struct TestGetVersionModule<'a> {
    name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sw_ver: Option<&'static str>,
    product_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sn: Option<&'static str>,
}

fn request_command(payload: serde_json::Value) -> PublishedMqttCommand {
    PublishedMqttCommand {
        topic: "device/01S00EXAMPLE/request".to_string(),
        payload,
        qos: BAMBU_MQTT_QOS,
    }
}

fn studio_sequence_id(payload: &serde_json::Value, section: &str) -> String {
    let envelope: TestSequenceEnvelope = decode_payload(payload);
    let sequence_id = &envelope.section(section).sequence_id;
    let parsed = sequence_id.parse::<u32>().unwrap();
    assert!((20000..30000).contains(&parsed));
    sequence_id.to_string()
}

fn decode_payload<T>(payload: &serde_json::Value) -> T
where
    T: for<'de> Deserialize<'de>,
{
    T::deserialize(payload).unwrap()
}

#[derive(Debug, Deserialize)]
struct TestSequenceEnvelope {
    info: Option<TestSequenceSection>,
    pushing: Option<TestSequenceSection>,
    print: Option<TestSequenceSection>,
    system: Option<TestSequenceSection>,
}

impl TestSequenceEnvelope {
    fn section(&self, section: &str) -> &TestSequenceSection {
        match section {
            "info" => self.info.as_ref(),
            "pushing" => self.pushing.as_ref(),
            "print" => self.print.as_ref(),
            "system" => self.system.as_ref(),
            _ => None,
        }
        .unwrap()
    }
}

#[derive(Debug, Deserialize)]
struct TestSequenceSection {
    sequence_id: String,
}

fn project_file_payload(payload: &serde_json::Value) -> TestProjectFilePayload {
    decode_payload(payload)
}

fn material_patch_json(json: &str) -> TestMaterialPatch {
    serde_json::from_str(json).unwrap()
}

fn chamber_light_payload(payload: &serde_json::Value) -> TestChamberLightPayload {
    decode_payload(payload)
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestChamberLightPayload {
    system: TestChamberLightSystem,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestChamberLightSystem {
    led_mode: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestProjectFilePayload {
    print: TestProjectFilePrint,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestProjectFilePrint {
    command: String,
    sequence_id: String,
    param: String,
    project_id: String,
    profile_id: String,
    task_id: String,
    subtask_id: String,
    subtask_name: String,
    url: String,
    file: String,
    md5: String,
    bed_type: String,
    bed_leveling: bool,
    flow_cali: bool,
    vibration_cali: bool,
    layer_inspect: bool,
    timelapse: bool,
    use_ams: bool,
    #[serde(default)]
    ams_mapping: Vec<i64>,
    #[serde(default)]
    ams_mapping2: Vec<TestAmsMapping2>,
    nozzle_mapping: Option<Vec<i32>>,
    ams_mapping_info: Option<Vec<TestAmsMappingInfo>>,
    auto_bed_leveling: u8,
    nozzle_offset_cali: u8,
    cfg: String,
    extrude_cali_flag: u8,
    extrude_cali_manual_mode: Option<i32>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsMapping2 {
    ams_id: i64,
    slot_id: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsMappingInfo {
    ams: i32,
    #[serde(rename = "targetColor")]
    target_color: String,
    #[serde(rename = "filamentId")]
    filament_id: String,
    #[serde(rename = "filamentType")]
    filament_type: String,
    #[serde(rename = "nozzleId")]
    nozzle_id: Option<i32>,
    #[serde(rename = "sourceColor")]
    source_color: Option<String>,
}

fn project_file_command() -> ProjectFileCommand {
    ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_owned(),
        url: None,
        md5: None,
        plate_id: 2,
        studio_submission_id: 38_191,
        submission_source: PrintSubmissionSource::Web,
        task_name: None,
        origin_profile_id: 0,
        use_ams: true,
        bed_leveling: false,
        auto_bed_leveling: PrintCalibrationMode::Off,
        flow_cali: false,
        vibration_cali: false,
        layer_inspect: false,
        auto_flow_cali: PrintCalibrationMode::Off,
        auto_offset_cali: PrintCalibrationMode::Off,
        timelapse: false,
        timelapse_use_internal: false,
        bed_type: "auto".to_owned(),
        extruder_cali_manual_mode: None,
        nozzle_mapping: Vec::new(),
        ams_mapping: Vec::new(),
        ams_mapping2: Vec::new(),
        ams_mapping_info: Vec::new(),
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialPatch {
    #[serde(rename = "type")]
    document_type: String,
    #[serde(default)]
    ams_units: Vec<TestMaterialUnit>,
    #[serde(default)]
    external_spools: Vec<TestExternalSpool>,
    active_tray: Option<TestActiveTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialUnit {
    #[serde(default)]
    trays: Vec<TestMaterialTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialTray {
    #[serde(rename = "type")]
    material_type: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestExternalSpool {
    external_id: String,
    filament_id: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TestActiveTray {
    Ams {
        global_tray_id: i64,
        ams_id: String,
        tray_id: String,
    },
    External {
        external_id: String,
        tray_id: String,
        global_tray_id: Option<u64>,
    },
}
