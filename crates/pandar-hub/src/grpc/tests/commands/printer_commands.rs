use super::*;

#[tokio::test]
async fn converts_refresh_printer_materials_command_to_proto() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = RefreshPrinterMaterialsPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "refresh_printer_materials".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command.unwrap() {
        hub_command::Command::RefreshPrinterMaterials(command) => {
            assert_eq!(command.printer_id, "printer-1");
            assert_eq!(command.serial_number, "SERIAL123");
        }
        other => panic!("expected refresh materials command, got {other:?}"),
    }
}

#[tokio::test]
async fn converts_reload_printer_connection_command_to_proto() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = ReloadPrinterConnectionPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "reload_printer_connection".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command.unwrap() {
        hub_command::Command::ReloadPrinterConnection(command) => {
            assert_eq!(command.printer_id, "printer-1");
            assert_eq!(command.serial_number, "SERIAL123");
        }
        other => panic!("expected reload printer connection command, got {other:?}"),
    }
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_printer_operation_home_axes() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::Home {
            axes: vec![PrinterAxis::X, PrinterAxis::Z],
            required_device_features: Vec::new(),
        },
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id,
        agent_id,
        printer_id: Some(printer_id),
        kind: "printer_operation".to_string(),
        status: "queued".to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let hub_command = hub_command_from_record(command).unwrap();

    match hub_command.command {
        Some(hub_command::Command::PrinterOperation(command)) => {
            assert_eq!(command.serial_number, "SERIAL123");
            match command.operation {
                Some(printer_operation::Operation::Home(home)) => {
                    assert_eq!(home.axes, vec![Axis::X as i32, Axis::Z as i32]);
                }
                other => panic!("expected home operation, got {other:?}"),
            }
        }
        other => panic!("expected printer operation command, got {other:?}"),
    }
}

#[test]
fn required_device_features_convert_to_proto_enum_values() {
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: Some("printer-1".to_owned()),
        kind: "printer_operation".to_owned(),
        status: "queued".to_owned(),
        payload_json: serde_json::json!({
            "printer_id": "printer-1",
            "serial_number": "SERIAL123",
            "operation": {
                "type": "home",
                "axes": [],
                "required_device_features": ["bambu_mqtt_homing"]
            }
        })
        .to_string(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    })
    .unwrap();

    let converted = hub_command_from_record(command).unwrap();
    let Some(hub_command::Command::PrinterOperation(operation)) = converted.command else {
        panic!("expected printer operation command");
    };
    assert_eq!(
        operation.required_device_features,
        [DeviceFeature::BambuMqttHoming as i32]
    );
}
