use pandar_core::H2cAutoNozzleMappingRequest;
use serde_json::json;

use super::*;

fn request() -> H2cAutoNozzleMappingRequest {
    serde_json::from_value(json!({
        "command": "get_auto_nozzle_mapping",
        "sequence_id": "42",
        "version": 1,
        "group_info": [{
            "id": 0,
            "ext": 1,
            "dia": 0.4,
            "vol": "E3D High Flow"
        }]
    }))
    .unwrap()
}

fn gateway_with_reports(
    reports: impl IntoIterator<Item = serde_json::Value>,
) -> ConfiguredBambuMachineGateway<FakeMqttTransport, FakeMachineFileTransfer> {
    ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(
            endpoint_without_model("SERIAL1"),
            FakeMqttTransport::with_reports(reports),
            FakeMachineFileTransfer::default(),
        )],
        Duration::from_secs(1),
        TransferModeCache::default(),
    )
}

#[tokio::test]
async fn auto_mapping_matches_command_and_sequence() {
    let gateway = gateway_with_reports([
        json!({
            "print": {
                "command": "pause",
                "sequence_id": "42",
                "result": "success"
            }
        }),
        json!({
            "print": {
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "42",
                "result": "success",
                "version": 1,
                "mapping": [16, 21]
            }
        }),
    ]);

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::GetAutoNozzleMapping(request()))
        .await
        .unwrap();
    let report = serde_json::to_value(result.mqtt_report.unwrap()).unwrap();
    assert_eq!(report["print"]["command"], "get_auto_nozzle_mapping");
    assert_eq!(report["print"]["mapping"], json!([16, 21]));
}

#[tokio::test]
async fn auto_mapping_preserves_correlated_printer_failure() {
    let gateway = gateway_with_reports([json!({
        "print": {
            "command": "get_auto_nozzle_mapping",
            "sequence_id": "42",
            "result": "failed",
            "version": "future",
            "reason": "rack busy",
            "errno": 17
        }
    })]);

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::GetAutoNozzleMapping(request()))
        .await
        .unwrap();
    let report = serde_json::to_value(result.mqtt_report.unwrap()).unwrap();
    assert_eq!(result.error.as_deref(), Some("rack busy"));
    assert_eq!(report["print"]["reason"], "rack busy");
    assert_eq!(report["print"]["errno"], 17);
}
