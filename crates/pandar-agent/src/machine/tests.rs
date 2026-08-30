use std::time::Duration;

use async_trait::async_trait;
use pandar_core::BambuDeviceFeatures;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, mpsc};

use super::*;
use crate::AgentConfig;
use crate::machine::{
    file_transfer::{FakeMachineFileTransfer, FileTransferRequest, PrintUploadPolicy},
    mqtt::{
        BAMBU_MQTT_QOS, BAMBU_MQTT_RETAIN, BambuMqttTransport, FakeMqttTransport, PrintErrorAction,
        PublishedMqttCommand,
    },
    print::pick_remote_name,
    runtime::test_support::{
        TestRuntimeBambuMachineGateway, assert_locked_for_a_moment, assert_unlocked_for_a_moment,
    },
};
use pandar_protocol::agent::v1::{HubCommand, RefreshPrinters, agent_event, hub_command};

mod axis_controls;
mod firmware_control;
mod firmware_generation;
mod firmware_reducer;
mod fixtures;
mod gateway;
mod link_report_ownership;
mod operations_ams;
mod operations_h2c;
mod operations_lights;
mod operations_motion;
mod print_error;
mod print_validation;
mod runtime;
mod studio_print_contract;

use fixtures::*;

fn endpoint(serial: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: serial.to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some(format!("printer-{serial}")),
    }
}

fn endpoint_without_model(serial: &str) -> BambuPrinterEndpoint {
    let mut endpoint = endpoint(serial);
    endpoint.model = None;
    endpoint
}

fn runtime_endpoint(serial: &str, name: &str, access_code: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: serial.to_string(),
        access_code: access_code.to_string(),
        model: Some("X1 Carbon".to_string()),
        name: Some(name.to_string()),
    }
}

fn get_version_report(model: &str) -> serde_json::Value {
    serde_json::to_value(TestGetVersionReport {
        info: TestGetVersionInfo {
            command: "get_version",
            module: [TestGetVersionModule {
                name: "ota",
                product_name: model,
            }],
        },
    })
    .unwrap()
}

fn runtime_state_report(state: &str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeStateReport {
        print: TestRuntimePrintReport {
            state,
            fun: None,
            ams: TestRuntimeAmsReport {
                ams: [TestRuntimeAmsUnit {
                    id: "0",
                    tray: [TestRuntimeAmsTray {
                        id: "0",
                        tray_type: "PLA",
                    }],
                }],
            },
        },
    })
    .unwrap()
}

fn runtime_feature_report(state: &str, fun: &'static str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeStateReport {
        print: TestRuntimePrintReport {
            state,
            fun: Some(fun),
            ams: TestRuntimeAmsReport {
                ams: [TestRuntimeAmsUnit {
                    id: "0",
                    tray: [TestRuntimeAmsTray {
                        id: "0",
                        tray_type: "PLA",
                    }],
                }],
            },
        },
    })
    .unwrap()
}

fn runtime_fun_only_report(fun: &str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeFunOnlyReport {
        print: TestRuntimeFunOnly { fun },
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
    module: [TestGetVersionModule<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestGetVersionModule<'a> {
    name: &'static str,
    product_name: &'a str,
}

#[derive(Debug, Serialize)]
struct TestRuntimeStateReport<'a> {
    print: TestRuntimePrintReport<'a>,
}

#[derive(Debug, Serialize)]
struct TestRuntimePrintReport<'a> {
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fun: Option<&'static str>,
    ams: TestRuntimeAmsReport,
}

#[derive(Debug, Serialize)]
struct TestRuntimeFunOnlyReport<'a> {
    print: TestRuntimeFunOnly<'a>,
}

#[derive(Debug, Serialize)]
struct TestRuntimeFunOnly<'a> {
    fun: &'a str,
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsReport {
    ams: [TestRuntimeAmsUnit; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsUnit {
    id: &'static str,
    tray: [TestRuntimeAmsTray; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsTray {
    id: &'static str,
    tray_type: &'static str,
}

fn runtime_reports(model: &str, state: &str) -> [serde_json::Value; 2] {
    [get_version_report(model), runtime_state_report(state)]
}

fn operation_report(value: impl Serialize) -> TestOperationReport {
    let value = serde_json::to_value(value).unwrap();
    serde::Deserialize::deserialize(value).unwrap()
}

#[derive(Debug, Deserialize)]
struct TestOperationReport {
    system: Option<TestOperationReportSection>,
    print: Option<TestOperationReportSection>,
}

#[derive(Debug, Deserialize)]
struct TestOperationReportSection {
    result: String,
}

fn runtime_transport(
    report_sets: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> FakeMqttTransport {
    FakeMqttTransport::with_reports(
        report_sets
            .into_iter()
            .flat_map(|(model, state)| runtime_reports(model, state)),
    )
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
    }
}

fn dynamic_sequence_id(payload: &Value) -> String {
    dynamic_section_sequence_id(payload, "print")
}

fn dynamic_project_file_submission_id(payload: &Value) -> String {
    let envelope: TestProjectFileEnvelope = decode_payload(payload);
    let submission_id = envelope.print.project_id;
    assert_ne!(submission_id, "0");
    assert!((1..=2_147_483_647).contains(&submission_id.parse::<u32>().unwrap()));
    assert_eq!(envelope.print.task_id, submission_id);
    assert_eq!(envelope.print.subtask_id, submission_id);
    submission_id
}

fn dynamic_section_sequence_id(payload: &Value, section: &str) -> String {
    let envelope: TestSequenceEnvelope = decode_payload(payload);
    let sequence_id = &envelope.section(section).sequence_id;
    assert_ne!(sequence_id, "0");
    assert!((20000..30000).contains(&sequence_id.parse::<u32>().unwrap()));
    sequence_id.to_string()
}

fn decode_payload<T>(payload: &Value) -> T
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

#[derive(Debug, Deserialize)]
struct TestProjectFileEnvelope {
    print: TestProjectFileSection,
}

#[derive(Debug, Deserialize)]
struct TestProjectFileSection {
    project_id: String,
    task_id: String,
    subtask_id: String,
}

fn print_project_file() -> PrintProjectFile {
    PrintProjectFile {
        job_id: "job-1".to_string(),
        artifact_id: "artifact-1".to_string(),
        printer_id: "printer-1".to_string(),
        serial_number: "SERIAL1".to_string(),
        filename: "plate.3mf".to_string(),
        storage_path: "tenant/artifact/plate.3mf".to_string(),
        artifact_download_path: "/api/v1/agents/agent-1/artifacts/artifact-1".to_string(),
        size_bytes: 3,
        plate_id: 1,
        studio_submission_id: 38_191,
        options: Some(pandar_protocol::agent::v1::PrintProjectFileOptions {
            use_ams: true,
            bed_leveling: false,
            flow_cali: false,
            vibration_cali: false,
            layer_inspect: false,
            record_timelapse: true,
            timelapse_use_internal: false,
            bed_type: "textured_plate".to_owned(),
            auto_bed_leveling: Some(0),
            auto_flow_cali: Some(0),
            auto_offset_cali: Some(0),
            extruder_cali_manual_mode: Some(-1),
            try_emmc_print: false,
            nozzle_mapping: Vec::new(),
            ams_mapping: Vec::new(),
            ams_mapping2: Vec::new(),
            ams_mapping_info: Vec::new(),
            nozzles_info: Vec::new(),
        }),
        task_metadata: Some(pandar_protocol::agent::v1::StudioTaskMetadata {
            task_name: "plate".to_owned(),
            project_name: String::new(),
            preset_name: String::new(),
            connection_type: "cloud".to_owned(),
            comments: String::new(),
            origin_profile_id: 0,
            stl_design_id: 0,
            origin_model_id: String::new(),
            print_type: "from_normal".to_owned(),
            submitted_device_name: String::new(),
            svc_context: String::new(),
            slicer_uid: String::new(),
        }),
        submission_source: pandar_protocol::agent::v1::PrintSubmissionSource::Studio as i32,
    }
}
