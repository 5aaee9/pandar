use std::{sync::atomic::AtomicU32, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::*;
use crate::machine::BambuPrinterEndpoint;
use crate::{
    AgentConfig,
    protocol::agent::v1::{PrintJobReport, agent_event},
};

mod fixtures;
mod hms;
mod snapshot;
mod tls;

use fixtures::*;

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: "01S00EXAMPLE".to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some("garage-a1".to_string()),
    }
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
    ams_mapping_info: Option<Vec<TestAmsMappingInfo>>,
    auto_bed_leveling: u8,
    nozzle_offset_cali: u8,
    cfg: String,
    extrude_cali_flag: u8,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsMapping2 {
    ams_id: i64,
    slot_id: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsMappingInfo {
    #[serde(rename = "nozzleId")]
    nozzle_id: i64,
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

#[test]
fn studio_command_payloads_use_incrementing_studio_sequence_ids() {
    let commands = [
        (BambuMqttCommand::GetVersion.payload(), "info"),
        (BambuMqttCommand::RequestPushAll.payload(), "pushing"),
        (BambuMqttCommand::PausePrint.payload(), "print"),
        (BambuMqttCommand::ResumePrint.payload(), "print"),
        (BambuMqttCommand::StopPrint.payload(), "print"),
        (BambuMqttCommand::SetChamberLight(true).payload(), "system"),
        (
            BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload(),
            "print",
        ),
        (
            BambuMqttCommand::GcodeLine(GcodeLineCommand {
                lines: vec!["G28".to_string()],
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::ProjectFile(ProjectFileCommand {
                printer_model: None,
                filename: "job.3mf".to_string(),
                url: None,
                md5: None,
                plate_id: 2,
                task_id: "task-1".to_string(),
                subtask_id: "subtask-1".to_string(),
                use_ams: true,
                flow_cali: true,
                timelapse: false,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsRereadRfid(AmsSlotCommand {
                ams_id: 0,
                slot_id: 1,
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsLoadFilament(AmsFilamentCommand {
                ams_id: 0,
                slot_id: 1,
                target: 1,
                extruder_id: Some(0),
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsUnloadFilament(AmsFilamentCommand {
                ams_id: 0,
                slot_id: 1,
                target: 1,
                extruder_id: None,
            })
            .payload(),
            "print",
        ),
    ];

    let command_count = commands.len();
    let mut sequence_ids = Vec::new();
    for (payload, section) in commands {
        sequence_ids.push(studio_sequence_id(&payload, section));
    }
    sequence_ids.sort();
    sequence_ids.dedup();
    assert_eq!(sequence_ids.len(), command_count);
}

#[test]
fn studio_sequence_id_wraps_before_leaving_studio_range() {
    let sequence = AtomicU32::new(29999);

    let last = next_studio_sequence_id_from(&sequence);
    let wrapped = next_studio_sequence_id_from(&sequence);
    let continued = next_studio_sequence_id_from(&sequence);

    assert_eq!(last, "29999");
    assert_eq!(wrapped, "20000");
    assert_eq!(continued, "20001");

    let out_of_range = AtomicU32::new(30000);
    assert_eq!(next_studio_sequence_id_from(&out_of_range), "20000");
    assert_eq!(next_studio_sequence_id_from(&out_of_range), "20001");
}

#[test]
fn topics_match_bambu_reference_shape() {
    let topics = BambuMqttTopics::for_serial("01S00EXAMPLE");

    assert_eq!(topics.report, "device/01S00EXAMPLE/report");
    assert_eq!(topics.request, "device/01S00EXAMPLE/request");
}

#[test]
fn constants_match_bambu_defaults() {
    assert_eq!(BAMBU_MQTT_PORT, 8883);
    assert_eq!(BAMBU_MQTT_USERNAME, "bblp");
    assert_eq!(BAMBU_MQTT_QOS, 1);
}

#[test]
fn lan_mqtt_accepts_full_pushall_reports() {
    let options = bambu_lan_mqtt_options(&endpoint(), None);

    assert!(options.max_packet_size() >= 256 * 1024);
}

#[test]
fn mqtt_report_error_log_preserves_error_chain() {
    let err = anyhow!("payload size limit exceeded: 262600")
        .context("MQTT serialization/deserialization error")
        .context("poll rumqttc event loop");

    let (logs, ()) = crate::test_tracing::capture_logs(|| warn_mqtt_report_receive_failed(&err));

    let captured = logs.contents();
    assert!(captured.contains("MQTT report receive failed"));
    assert!(captured.contains("payload size limit exceeded: 262600"));
    assert!(captured.contains("poll rumqttc event loop"));
}

#[test]
fn lan_tls_uses_rustls_certificate_policy_for_printer_certificates() {
    assert!(matches!(
        bambu_lan_tls_config(),
        TlsConfiguration::Rustls(_)
    ));
}

#[test]
fn ftps_lan_tls_default_profile_config_constructs() {
    let config = crate::machine::ftps::bambu_lan_ftps_tls_config_for_default_profile();

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn pushall_payload_matches_reference() {
    let payload = BambuMqttCommand::RequestPushAll.payload();
    let sequence_id = studio_sequence_id(&payload, "pushing");
    assert_eq!(payload, expected_pushall_payload(&sequence_id));
}

#[test]
fn get_version_payload_matches_reference() {
    let payload = BambuMqttCommand::GetVersion.payload();
    let sequence_id = studio_sequence_id(&payload, "info");
    assert_eq!(payload, expected_get_version_payload(&sequence_id));
}

#[test]
fn get_version_report_extracts_trimmed_ota_model() {
    assert_eq!(
        model_from_get_version_report(
            parse_get_version_report(&get_version_report(" P2S ")).unwrap()
        )
        .unwrap(),
        "P2S"
    );
}

#[test]
fn get_version_report_rejects_missing_model() {
    let report = get_version_report_with_blank_model();

    assert!(model_from_get_version_report(parse_get_version_report(&report).unwrap()).is_err());
}

#[test]
fn basic_print_control_payloads_match_reference() {
    let pause = BambuMqttCommand::PausePrint.payload();
    let resume = BambuMqttCommand::ResumePrint.payload();
    let stop = BambuMqttCommand::StopPrint.payload();
    assert_eq!(
        pause,
        expected_print_command_payload("pause", "", &studio_sequence_id(&pause, "print"))
    );
    assert_eq!(
        resume,
        expected_print_command_payload("resume", "", &studio_sequence_id(&resume, "print"))
    );
    assert_eq!(
        stop,
        expected_print_command_payload("stop", "", &studio_sequence_id(&stop, "print"))
    );
}

#[test]
fn chamber_light_payload_matches_bambu_studio_reference() {
    let on = BambuMqttCommand::SetChamberLight(true).payload();
    assert_eq!(
        on,
        expected_chamber_light_payload(&studio_sequence_id(&on, "system"))
    );

    let off = BambuMqttCommand::SetChamberLight(false).payload();
    assert_eq!(chamber_light_payload(&off).system.led_mode, "off");
}

#[test]
fn print_speed_is_limited_to_reference_modes() {
    let payload = BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload();
    assert_eq!(
        payload,
        expected_print_command_payload("print_speed", "4", &studio_sequence_id(&payload, "print"))
    );
    assert!(PrintSpeed::new(0).is_err());
    assert!(PrintSpeed::new(5).is_err());
}

#[test]
fn select_extruder_payload_matches_bambu_studio_reference() {
    let payload = BambuMqttCommand::SelectExtruder(1).payload();
    assert_eq!(
        payload,
        expected_select_extruder_payload(1, &studio_sequence_id(&payload, "print"))
    );
}

#[test]
fn gcode_line_payload_preserves_single_home_line() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        lines: vec!["G28".to_string()],
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload("gcode_line", "G28", &studio_sequence_id(&payload, "print"))
    );
}

#[test]
fn gcode_line_payload_joins_relative_move_lines() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        lines: vec![
            "G91".to_string(),
            "G0 X10 Z-0.5 F3000".to_string(),
            "G90".to_string(),
        ],
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload(
            "gcode_line",
            "G91\nG0 X10 Z-0.5 F3000\nG90",
            &studio_sequence_id(&payload, "print")
        )
    );
}

#[test]
fn gcode_line_payload_preserves_hotend_temperature_line() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        lines: vec!["M104 S200".to_string()],
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload(
            "gcode_line",
            "M104 S200",
            &studio_sequence_id(&payload, "print")
        )
    );
}

#[test]
fn raw_json_payload_is_preserved() {
    let payload = raw_print_payload("custom", "9");
    assert_eq!(
        BambuMqttCommand::RawJson(payload.clone()).payload(),
        payload
    );
}

#[test]
fn project_file_payload_reserves_dispatch_identity_and_flags() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: true,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    })
    .payload();

    let sequence_id = studio_sequence_id(&payload, "print");
    let print = project_file_payload(&payload).print;
    assert_eq!(print.command, "project_file");
    assert_eq!(print.sequence_id, sequence_id);
    assert_eq!(print.param, "Metadata/plate_2.gcode");
    assert_eq!(print.profile_id, "0");
    assert_eq!(print.subtask_name, "job");
    assert_eq!(print.url, "ftp://job.3mf");
    assert_eq!(print.file, "job.3mf");
    assert_eq!(print.md5, "");
    assert_eq!(print.bed_type, "auto");
    assert!(!print.bed_leveling);
    assert!(print.flow_cali);
    assert!(!print.vibration_cali);
    assert!(!print.layer_inspect);
    assert!(!print.timelapse);
    assert!(print.use_ams);
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert_eq!(print.ams_mapping_info, None);
    assert_eq!(print.auto_bed_leveling, 0);
    assert_eq!(print.nozzle_offset_cali, 0);
    assert_eq!(print.cfg, "0");
    assert_eq!(print.extrude_cali_flag, 0);

    let project_id = print.project_id.parse::<u32>().unwrap();
    assert!((1..=2_147_483_647).contains(&project_id));
    assert_eq!(print.task_id, print.project_id);
    assert_eq!(print.subtask_id, print.project_id);
}

#[test]
fn project_file_payload_defaults_mapping_keys_when_no_mapping_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: false,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert!(!print.use_ams);
}

#[test]
fn project_file_payload_includes_ams_mapping_only_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[0,-1,4]".to_string()),
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, vec![0, -1, 4]);
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert!(print.use_ams);
}

#[test]
fn project_file_payload_includes_ams_mapping2_only_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: Some(r#"[{"ams_id":255,"slot_id":0}]"#.to_string()),
        ams_mapping_info_json: None,
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(
        print.ams_mapping2,
        vec![TestAmsMapping2 {
            ams_id: 255,
            slot_id: 0
        }]
    );
}

#[test]
fn project_file_payload_includes_both_mapping_keys_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[0,1]".to_string()),
        ams_mapping2_json: Some(r#"[{"ams_id":0,"slot_id":1}]"#.to_string()),
        ams_mapping_info_json: Some(r#"[{"nozzleId":0},{"nozzleId":1}]"#.to_string()),
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, vec![0, 1]);
    assert_eq!(
        print.ams_mapping2,
        vec![TestAmsMapping2 {
            ams_id: 0,
            slot_id: 1
        }]
    );
    assert_eq!(
        print.ams_mapping_info,
        Some(vec![
            TestAmsMappingInfo { nozzle_id: 0 },
            TestAmsMappingInfo { nozzle_id: 1 }
        ])
    );
}

#[test]
fn project_file_payload_rewrites_flat_external_mapping_values() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        printer_model: None,
        filename: "job.3mf".to_string(),
        url: None,
        md5: None,
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[254,255,15]".to_string()),
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    })
    .payload();

    assert_eq!(
        project_file_payload(&payload).print.ams_mapping,
        vec![-1, -1, 15]
    );
}

#[tokio::test]
async fn refresh_subscribes_publishes_and_maps_report() {
    let mut endpoint = endpoint();
    endpoint.model = Some("Configured Model".to_string());
    let transport = FakeMqttTransport::with_reports([
        get_version_report("P2S"),
        print_gcode_state_report("RUNNING"),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(
        refreshed.snapshot,
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("12345678".to_string()),
            name: "garage-a1".to_string(),
            model: Some("P2S".to_string()),
            state: "RUNNING".to_string(),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_light_on: None,
        }
    );
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
    let published = transport.published_commands().await;
    let get_version_sequence_id = studio_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = studio_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            request_command(expected_get_version_payload(&get_version_sequence_id)),
            request_command(expected_pushall_payload(&pushall_sequence_id)),
        ]
    );
}

#[tokio::test]
async fn refresh_printer_returns_material_patch_when_pushall_report_has_ams() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        ams_print_report("IDLE", "PLA", Some("FF0000"), Some("0")),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.serial, "01S00EXAMPLE");
    let materials = refreshed.materials.unwrap();
    let patch = material_patch_json(&materials.printer_materials_json);
    assert_eq!(patch.document_type, "printer_material_patch");
    assert_eq!(
        patch.ams_units[0].trays[0].material_type.as_deref(),
        Some("PLA")
    );
}

#[tokio::test]
async fn refresh_printer_keeps_first_snapshot_and_continues_until_ams_patch() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        print_gcode_state_report("IDLE"),
        ams_print_report("IDLE", "PLA", None, None),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.state, "IDLE");
    assert!(refreshed.materials.is_some());
}

#[tokio::test]
async fn material_refresh_uses_total_deadline_for_infinite_non_ams_reports() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer_materials(&transport, &endpoint(), None, Duration::from_millis(10))
        .await
        .unwrap_err();

    let error = format!("{err:#}");
    assert!(error.contains("no AMS material report received before timeout"));
}

#[tokio::test]
async fn refresh_ignores_unrelated_reports_before_get_version() {
    let transport = FakeMqttTransport::with_reports([
        print_gcode_state_report("STALE"),
        info_command_report("other"),
        get_version_report("X1 Carbon"),
        print_state_report("READY"),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(refreshed.snapshot.state, "READY");
    let published = transport.published_commands().await;
    let get_version_sequence_id = studio_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = studio_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            request_command(expected_get_version_payload(&get_version_sequence_id)),
            request_command(expected_pushall_payload(&pushall_sequence_id)),
        ]
    );
}

#[tokio::test]
async fn refresh_timeout_error_includes_serial_context() {
    let transport = FakeMqttTransport::with_timeout();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("refresh printer 01S00EXAMPLE"));
    assert!(format!("{err:#}").contains("wait for MQTT get_version report"));
    assert!(transport.published_commands().await.len() == 1);
}

#[tokio::test]
async fn refresh_fails_total_get_version_deadline_when_unrelated_reports_continue() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(10))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("timed out waiting for MQTT get_version report"));
    let published = transport.published_commands().await;
    let sequence_id = studio_sequence_id(&published[0].payload, "info");
    assert_eq!(
        published,
        [request_command(expected_get_version_payload(&sequence_id))]
    );
}

#[tokio::test]
async fn refresh_missing_model_fails_before_pushall() {
    let transport = FakeMqttTransport::with_reports([get_version_report_with_blank_model()]);

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("missing ota product_name"));
    let published = transport.published_commands().await;
    let sequence_id = studio_sequence_id(&published[0].payload, "info");
    assert_eq!(
        published,
        [request_command(expected_get_version_payload(&sequence_id))]
    );
}

#[tokio::test]
async fn refresh_get_version_publish_failure_fails_before_pushall() {
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("publish get_version"));
    assert!(format!("{err:#}").contains("fake publish failure"));
    assert!(transport.published_commands().await.is_empty());
}

#[test]
fn refresh_discovery_failure_log_includes_serial_and_error_chain() {
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let (logs, ()) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
                    .await
                    .unwrap_err();
            });
    });

    let captured = logs.contents();
    assert!(captured.contains("printer model discovery failed"));
    assert!(captured.contains("01S00EXAMPLE"));
    assert!(captured.contains("publish get_version to request topic"));
    assert!(captured.contains("fake publish failure"));
}

#[test]
fn print_report_from_report_extracts_progress_and_diagnostics() {
    let report = detailed_progress_report();

    let progress = print_report_from_report(&endpoint(), &report);

    assert_eq!(progress.serial, "01S00EXAMPLE");
    assert_eq!(progress.job_id.as_deref(), Some("job-123"));
    assert_eq!(progress.artifact_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.subtask_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(progress.percent, Some(42));
    assert_eq!(progress.remaining_time_minutes, Some(87));
    assert_eq!(progress.current_layer, Some(12));
    assert_eq!(progress.total_layers, Some(120));
    assert_eq!(progress.gcode_file.as_deref(), Some("plate_1.gcode"));
    assert_eq!(progress.subtask_name.as_deref(), Some("drawer-organizer"));
    assert_eq!(progress.diagnostics.len(), 2);
    assert_eq!(progress.diagnostics[0].kind, "print_error");
    assert_eq!(progress.diagnostics[0].severity, "error");
    assert_eq!(progress.diagnostics[0].message, "nozzle temperature error");
    assert_eq!(progress.diagnostics[1].kind, "hms");
    assert_eq!(progress.diagnostics[1].severity, "warning");
    assert_eq!(
        progress.diagnostics[1].code.as_deref(),
        Some("0300_0A00_0001_0002")
    );
    assert_eq!(progress.diagnostics[1].message, "fan speed is low");
    assert!(!progress.observed_at.is_empty());
}

#[test]
fn print_report_diagnostic_payload_includes_raw_print_report() {
    let report = serde_json::json!({
        "print": {
            "gcode_state": "FAILED",
            "mc_percent": 0,
            "print_error": 0,
            "reason": "reject_nozzle_mismatch",
            "result": "fail"
        }
    });

    let progress = print_report_from_report(&endpoint(), &report);

    assert_eq!(progress.diagnostics.len(), 1);
    let payload = serde_json::to_value(&progress.diagnostics[0].payload).unwrap();
    assert_eq!(payload["print_error"], 0);
    assert_eq!(payload["raw_print"]["gcode_state"], "FAILED");
    assert_eq!(payload["raw_print"]["reason"], "reject_nozzle_mismatch");
    assert_eq!(payload["raw_print"]["result"], "fail");
}

#[test]
fn print_report_from_report_drops_out_of_range_numeric_values() {
    let report = out_of_range_progress_report();

    let progress = print_report_from_report(&endpoint(), &report);

    assert_eq!(progress.percent, None);
    assert_eq!(progress.remaining_time_minutes, None);
    assert_eq!(progress.current_layer, None);
    assert_eq!(progress.total_layers, None);
}

#[test]
fn print_job_report_event_sets_numeric_presence_booleans() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let progress = PrintReportProgress {
        serial: "01S00EXAMPLE".to_owned(),
        job_id: Some("job-123".to_owned()),
        artifact_id: None,
        subtask_id: None,
        gcode_state: Some("RUNNING".to_owned()),
        percent: Some(0),
        remaining_time_minutes: None,
        current_layer: Some(7),
        total_layers: None,
        gcode_file: None,
        subtask_name: None,
        hms: None,
        diagnostics: Vec::new(),
        observed_at: "2026-06-22T00:00:00Z".to_owned(),
        printer_materials_json: String::new(),
    };

    let event = print_job_report_event(&config, progress);

    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    let Some(agent_event::Event::PrintJobReport(PrintJobReport {
        percent,
        has_percent,
        remaining_time_minutes,
        has_remaining_time_minutes,
        current_layer,
        has_current_layer,
        total_layers,
        has_total_layers,
        hms,
        has_hms,
        printer_materials_json,
        ..
    })) = event.event
    else {
        panic!("expected print job report event");
    };
    assert_eq!(percent, 0);
    assert!(has_percent);
    assert_eq!(remaining_time_minutes, 0);
    assert!(!has_remaining_time_minutes);
    assert_eq!(current_layer, 7);
    assert!(has_current_layer);
    assert_eq!(total_layers, 0);
    assert!(!has_total_layers);
    assert!(hms.is_empty());
    assert!(!has_hms);
    assert!(printer_materials_json.is_empty());
}

#[test]
fn print_report_from_report_populates_printer_materials_json() {
    let report = external_vt_tray_report(254, "GFL05", "#abcdef");

    let progress = print_report_from_report(&endpoint(), &report);
    let materials = material_patch_json(&progress.printer_materials_json);

    assert_eq!(materials.external_spools[0].external_id, "254");
    assert_eq!(
        materials.external_spools[0].filament_id.as_deref(),
        Some("GFL05")
    );
    assert_eq!(
        materials.external_spools[0].color.as_deref(),
        Some("ABCDEF")
    );
    assert_eq!(
        materials.active_tray,
        Some(TestActiveTray::External {
            external_id: "254".to_owned(),
            tray_id: "0".to_owned(),
            global_tray_id: None,
        })
    );
}

#[tokio::test]
async fn forward_print_reports_uses_transport_without_live_socket() {
    let transport = FakeMqttTransport::with_reports([print_job_progress_report(
        "job-123",
        "artifact-456",
        "RUNNING",
        55,
    )]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let forwarder = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(1),
                &sender,
            )
            .await
        }
    });

    let event = receiver.recv().await.unwrap();
    drop(receiver);
    forwarder.await.unwrap().unwrap();

    let Some(agent_event::Event::PrintJobReport(report)) = event.event else {
        panic!("expected print job report event");
    };
    assert_eq!(report.serial, "01S00EXAMPLE");
    assert_eq!(report.job_id, "job-123");
    assert_eq!(report.artifact_id, "artifact-456");
    assert_eq!(report.subtask_id, "artifact-456");
    assert_eq!(report.percent, 55);
    assert!(report.has_percent);
    assert!(report.printer_materials_json.is_empty());
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
}

#[tokio::test]
async fn forward_print_reports_emits_printer_snapshot_with_temperatures() {
    let transport =
        FakeMqttTransport::with_reports([print_temperature_report("RUNNING", 41, 220, 60, 32)]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let task = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(50),
                &sender,
            )
            .await
            .unwrap();
        }
    });

    assert!(matches!(
        receiver.recv().await.unwrap().event,
        Some(agent_event::Event::PrintJobReport(_))
    ));
    let second = timeout(Duration::from_millis(50), receiver.recv())
        .await
        .expect("expected printer snapshot event")
        .unwrap();
    task.abort();

    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = second.event else {
        panic!("expected printer snapshot event");
    };
    assert_eq!(snapshot.serial, "01S00EXAMPLE");
    assert_eq!(snapshot.model, "A1 Mini");
    assert_eq!(snapshot.state, "RUNNING");
    assert_eq!(snapshot.nozzle_temperatures[0].current_celsius, "41");
    assert_eq!(snapshot.nozzle_temperatures[0].target_celsius, "220");
    assert_eq!(snapshot.bed_temperature_celsius, "60");
    assert_eq!(snapshot.chamber_temperature_celsius, "32");
}

#[tokio::test]
async fn forward_print_reports_emits_material_snapshot_for_unsolicited_ams_report() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let transport = FakeMqttTransport::with_reports([ams_print_report("IDLE", "PLA", None, None)]);
    let (sender, mut receiver) = mpsc::channel(2);

    let task = tokio::spawn(async move {
        forward_print_reports(
            &config,
            &transport,
            &endpoint(),
            Duration::from_millis(50),
            &sender,
        )
        .await
        .unwrap();
    });

    let first = receiver.recv().await.unwrap();
    assert!(matches!(
        first.event,
        Some(agent_event::Event::PrintJobReport(_))
    ));
    let second = receiver.recv().await.unwrap();
    assert_material_snapshot(second, "01S00EXAMPLE", None);
    task.abort();
}

fn assert_material_snapshot(event: AgentEvent, serial: &str, printer_id: Option<&str>) {
    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    match event.event.unwrap() {
        agent_event::Event::PrinterMaterialsSnapshot(snapshot) => {
            assert_eq!(snapshot.serial, serial);
            assert_eq!(snapshot.printer_id, printer_id.unwrap_or_default());
            let patch = material_patch_json(&snapshot.printer_materials_json);
            assert_eq!(patch.document_type, "printer_material_patch");
            assert_eq!(
                patch.ams_units[0].trays[0].material_type.as_deref(),
                Some("PLA")
            );
        }
        other => panic!("expected printer materials snapshot, got {other:?}"),
    }
}
