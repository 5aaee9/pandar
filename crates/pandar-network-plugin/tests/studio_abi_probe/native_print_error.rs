use serde::Deserialize;

use super::{
    mock_hub::MockMode,
    run::{ProbeOutput, run_probe},
};

#[derive(Deserialize)]
struct NativeProbeResult {
    ok: bool,
    cloud_version_messages: u32,
    cloud_status_messages: u32,
    local_version_messages: u32,
    local_status_messages: u32,
    printer_connected_callbacks: u32,
    local_connect_callbacks: u32,
    operation_posts: u32,
    status_requests_zero_posts: bool,
    status_request_lookalikes_exact: bool,
    native_actions_exact: bool,
    cloud_unsupported_exact: bool,
    local_unsupported_exact: bool,
    cloud_invalid_native_exact: bool,
    local_invalid_native_exact: bool,
    cloud_requests_exact: bool,
    lan_connect_exact: bool,
    local_requests_exact: bool,
    callback_tunnels_exact: bool,
    local_only_refresh_exact: bool,
    local_replacement_exact: bool,
    disconnect_exact: bool,
    same_serial_exact: bool,
    cloud_status_body: String,
    local_status_body: String,
}

#[test]
fn studio_abi_probe_routes_native_status_through_explicit_tunnels() {
    let ProbeOutput {
        stdout,
        stderr,
        compiler,
    } = run_probe(MockMode::NativePrintError, "native-print-error");

    println!("Studio ABI probe compiler: {compiler}");
    assert!(!compiler.trim().is_empty());
    assert!(
        stderr.is_empty(),
        "native probe stderr was not empty: {stderr}"
    );
    let result: NativeProbeResult = serde_json::from_str(stdout.trim()).unwrap();
    assert!(result.ok);
    assert_eq!(
        (
            result.cloud_requests_exact,
            result.lan_connect_exact,
            result.local_requests_exact,
            result.callback_tunnels_exact,
            result.local_only_refresh_exact,
            result.local_replacement_exact,
            result.disconnect_exact,
            result.same_serial_exact,
        ),
        (true, true, true, true, true, true, true, true)
    );
    assert!(result.cloud_version_messages > 0);
    assert!(result.cloud_status_messages > 0);
    assert!(result.local_version_messages > 0);
    assert!(result.local_status_messages > 0);
    assert!(result.printer_connected_callbacks > 0);
    assert_eq!(result.local_connect_callbacks, 2);
    assert_eq!(result.operation_posts, 6);
    assert!(result.status_requests_zero_posts);
    assert!(result.status_request_lookalikes_exact);
    assert!(result.native_actions_exact);
    assert!(result.cloud_unsupported_exact);
    assert!(result.local_unsupported_exact);
    assert!(result.cloud_invalid_native_exact);
    assert!(result.local_invalid_native_exact);

    let cloud: serde_json::Value = serde_json::from_str(&result.cloud_status_body).unwrap();
    let cloud = &cloud["print"];
    assert_eq!(cloud["fun"], serde_json::json!("8000004100000020"));
    assert_eq!(cloud["print_error"], serde_json::json!(83_918_929));
    assert_eq!(cloud["job_id"], serde_json::json!("job-7"));
    assert_eq!(cloud["mc_percent"], serde_json::json!(37));
    assert_eq!(cloud["hms"][0]["code"], serde_json::json!(32_785));
    assert_eq!(cloud["ams"]["ams"][0]["tray"][0]["tray_type"], "PETG-CF");

    let local: serde_json::Value = serde_json::from_str(&result.local_status_body).unwrap();
    let local = &local["print"];
    assert_eq!(local["fun"], serde_json::json!("8000004100000020"));
    assert_eq!(local["print_error"], serde_json::json!(0));
    assert_eq!(local["job_id"], serde_json::json!(""));
    assert_eq!(local["mc_percent"], serde_json::json!(37));
    assert_eq!(local["hms"][0]["attr"], serde_json::json!(134_152_704));
    assert_eq!(local["ams"]["ams"][1]["tray"][3]["tray_type"], "PLA-CF");
}
