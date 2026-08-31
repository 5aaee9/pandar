#![cfg(any(unix, windows))]

#[path = "studio_projection_contract/compiler.rs"]
mod compiler;
#[path = "studio_projection_contract/pinned.rs"]
mod pinned;

use pandar_network_plugin::studio_status::project_hub_printers;

fn telemetry(printer: &str) -> serde_json::Value {
    let fields = serde_json::from_str::<serde_json::Value>(printer).unwrap();
    let mut device = serde_json::json!({
        "dev_id": "studio-serial-1",
        "pandar_printer_id": "printer-1",
        "dev_online": true,
        "online": true,
        "firmware": null
    });
    device
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let body = serde_json::json!({"message": "success", "devices": [device]}).to_string();
    let projection = project_hub_printers(&body).unwrap();
    let status =
        serde_json::from_str::<serde_json::Value>(projection.printers()[0].status_report())
            .unwrap();
    status["print"].clone()
}

fn projected(raw: &str) -> (serde_json::Value, serde_json::Value) {
    let telemetry = telemetry(raw);
    let consumed = compiler::run(&telemetry.to_string());
    (telemetry, consumed)
}

#[test]
fn compiled_pinned_studio_consumers_reject_unavailable_projection_capabilities() {
    assert_eq!(
        pinned::STUDIO_COMMIT,
        "42d319c6692fa8e64790fddf0cdaafd2a4254bcc"
    );
    let (telemetry, consumed) = projected(
        r#"{"fun":"FFFFFFFFFFFFFFFF","chamber_temperature_celsius":"32","materials":{"cfg":"FFFFFFFFFFFFFFFF","aux":"00002000","stat":"0","ams_units":[]}}"#,
    );

    assert_eq!(telemetry["sdcard"], false);
    assert_eq!(telemetry["support_chamber"], false);
    assert_eq!(telemetry["chamber_temper"], 32);
    assert_eq!(telemetry["ctt"], 0);
    assert_eq!(consumed["gate"], true);
    assert_eq!(consumed["sdcard_state"], 2);
    assert_eq!(consumed["camera_hidden"], true);
    assert_eq!(consumed["chamber"], false);
    assert_eq!(consumed["chamber_display"], false);
    assert_eq!(consumed["snapshot_detection"], false);
    assert_eq!(consumed["unsupported_fun_hidden"], true);
    assert_eq!(consumed["ext_change_assist_supported"], false);
    assert_eq!(consumed["axis_homing"], true);
    assert_eq!(consumed["axis_control"], true);
}

#[test]
fn compiled_pinned_studio_consumers_accept_known_supported_projection_cases() {
    let (telemetry, consumed) = projected(
        r#"{"fun":"4100000000","chamber_temperature_celsius":"32","chamber_target_temperature_celsius":"45","materials":{"cfg":"1","aux":"00001000","stat":"0","ams_units":[]}}"#,
    );

    assert_eq!(telemetry["sdcard"], true);
    assert_eq!(telemetry["support_chamber"], true);
    assert_eq!(telemetry["chamber_temper"], 32);
    assert_eq!(telemetry["ctt"], 45);
    assert_eq!(consumed["gate"], true);
    assert_eq!(consumed["sdcard_state"], 1);
    assert_eq!(consumed["camera_hidden"], true);
    assert_eq!(consumed["chamber"], true);
    assert_eq!(consumed["chamber_display"], true);
    assert_eq!(consumed["axis_homing"], true);
    assert_eq!(consumed["axis_control"], true);
}

#[test]
fn pinned_wtm_capability_is_visible_upstream_and_hidden_by_the_plugin() {
    let raw = r#"{"fun":"1000000000000000","cfg":"","aux":"0","stat":"0"}"#;
    let upstream = compiler::run(raw);
    assert_eq!(upstream["nozzle_rack_supported"], true);

    let (telemetry, projected) = projected(raw);
    assert_eq!(telemetry["fun"], "0");
    assert_eq!(projected["nozzle_rack_supported"], false);
}

#[test]
fn pinned_external_change_assist_is_visible_upstream_and_hidden_by_the_plugin() {
    let raw = r#"{"fun":"1000000000000","cfg":"","aux":"0","stat":"0"}"#;
    let upstream = compiler::run(raw);
    assert_eq!(upstream["ext_change_assist_supported"], true);

    let (telemetry, projected) = projected(raw);
    assert_eq!(telemetry["fun"], "0");
    assert_eq!(projected["ext_change_assist_supported"], false);
}

#[test]
fn compiled_pinned_studio_consumer_observes_cloud_connection_type() {
    let (telemetry, consumed) =
        projected(r#"{"materials":{"cfg":"1","aux":"00001000","stat":"0","ams_units":[]}}"#);

    assert_eq!(telemetry["device"]["connection_type"], "cloud");
    assert_eq!(consumed["connection_type"], "cloud");
}

#[test]
fn pinned_emmc_capability_is_visible_upstream_and_masked_to_bit_zero_by_the_plugin() {
    let raw = r#"{"fun":"1","fun2":"80000000000000A3","cfg":"","aux":"0","stat":"0"}"#;

    let upstream = compiler::run(raw);
    assert_eq!(upstream["emmc_print_supported"], true);
    assert_eq!(upstream["remote_dry_supported"], true);
    assert_eq!(upstream["active_arc_fitting_supported"], false);
    assert_eq!(upstream["pa_mode_supported"], false);
    assert_eq!(upstream["model_internal_storage_supported"], false);
    assert_eq!(upstream["ams_preload_version"], 0);

    let (telemetry, projected) = projected(raw);
    assert_eq!(telemetry["fun2"], "1");
    assert_eq!(projected["emmc_print_supported"], true);
    assert_eq!(projected["pa_mode_supported"], false);
    assert_eq!(projected["remote_dry_supported"], false);
    assert_eq!(projected["active_arc_fitting_supported"], false);
    assert_eq!(projected["model_internal_storage_supported"], false);
    assert_eq!(projected["ams_preload_version"], 0);
}

#[test]
fn pinned_studio_consumer_fails_closed_without_an_emmc_observation() {
    let (telemetry, consumed) =
        projected(r#"{"fun":"FFFFFFFFFFFFFFFF","cfg":"","aux":"0","stat":"0"}"#);

    assert!(telemetry.get("fun2").is_none());
    assert_eq!(consumed["emmc_print_supported"], false);
}

#[test]
fn projected_emmc_capability_reports_unset_bit_zero() {
    let (telemetry, consumed) =
        projected(r#"{"fun2":"80000000000000A0","cfg":"","aux":"0","stat":"0"}"#);

    assert_eq!(telemetry["fun2"], "0");
    assert_eq!(consumed["emmc_print_supported"], false);
}
