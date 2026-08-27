use super::*;

#[tokio::test]
async fn native_print_error_dispatch_preserves_transport_and_result_correlation() {
    let mqtt = FakeMqttTransport::with_operation_reports();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(
            endpoint_without_model("01S00EXAMPLE"),
            mqtt.clone(),
            transfer,
        )],
        Duration::from_secs(1),
    );

    let result = gateway
        .operate_printer(
            "01S00EXAMPLE",
            PrinterOperation::HandlePrintError {
                error_action: PrintErrorAction::Ignore,
                print_error: 83_918_929,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        mqtt.subscriptions().await,
        vec!["device/01S00EXAMPLE/report".to_owned()]
    );
    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/01S00EXAMPLE/request".to_owned(),
            payload: serde_json::json!({
                "print": {
                    "command": "ignore",
                    "err": "83918929",
                    "job_id": "job-7",
                    "param": "reserve",
                    "sequence_id": "20042"
                }
            }),
            qos: BAMBU_MQTT_QOS,
        }]
    );
    const { assert!(!BAMBU_MQTT_RETAIN) };
    assert_eq!(result.sequence_id.as_deref(), Some("20042"));
    assert_eq!(
        operation_report(result.mqtt_report.as_ref().unwrap())
            .print
            .unwrap()
            .result,
        "success"
    );
    assert_eq!(result.error, None);
}
