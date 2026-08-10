use super::*;

#[tokio::test]
async fn grpc_hub_command_from_record_maps_printer_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetPrintSpeed { speed_mode: 4 },
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetPrintSpeed(speed))
                        if speed.speed_mode == 4
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_fan_speed_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetFanSpeed {
            fan_index: 3,
            speed_percent: 50,
            airduct: true,
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if matches!(
                command.operation,
                Some(printer_operation::Operation::SetFanSpeed(fan))
                    if fan.fan_index == 3 && fan.speed_percent == 50 && fan.airduct
            )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_toggle_light_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::ToggleLight {},
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(command.operation, Some(printer_operation::Operation::ToggleLight(_)))
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_set_chamber_light_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetChamberLight { on: true },
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetChamberLight(operation)) if operation.on
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_select_extruder_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SelectExtruder { extruder_id: 1 },
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SelectExtruder(operation))
                        if operation.extruder_id == 1
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_targeted_hotend_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetHotendTemperature {
            temperature_celsius: 220,
            wait: false,
            extruder_id: Some(1),
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetHotendTemperature(operation))
                        if operation.temperature_celsius == 220 && operation.extruder_id == Some(1)
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_bed_temperature_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetBedTemperature {
            temperature_celsius: 75,
            wait: false,
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetBedTemperature(operation))
                        if operation.temperature_celsius == 75
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_chamber_temperature_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::SetChamberTemperature {
            temperature_celsius: 45,
            wait: false,
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

    assert!(matches!(
        hub_command.command,
        Some(hub_command::Command::PrinterOperation(command))
            if command.serial_number == "SERIAL123"
                && matches!(
                    command.operation,
                    Some(printer_operation::Operation::SetChamberTemperature(operation))
                        if operation.temperature_celsius == 45
                )
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_ams_slot_operation() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let printer_id = "printer-1".to_string();
    let payload = PrinterOperationPayload {
        printer_id: printer_id.clone(),
        serial_number: "SERIAL123".to_string(),
        operation: PrinterOperationKind::AmsLoadFilament {
            ams_id: 0,
            slot_id: 1,
            global_tray_id: Some(1),
            external_id: None,
            extruder_id: Some(0),
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
                Some(printer_operation::Operation::AmsLoadFilament(operation)) => {
                    assert_eq!(operation.ams_id, 0);
                    assert_eq!(operation.slot_id, 1);
                    assert_eq!(operation.global_tray_id, 1);
                    assert_eq!(operation.external_id, "");
                    assert_eq!(operation.extruder_id, Some(0));
                }
                other => panic!("expected AMS load operation, got {other:?}"),
            }
        }
        other => panic!("expected printer operation command, got {other:?}"),
    }
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_h2c_rack_operations() {
    for (operation, assert_operation) in [
        (
            PrinterOperationKind::NozzleHolderCtrl { action: 2 },
            (|operation: &Option<printer_operation::Operation>| {
                assert!(matches!(
                    operation,
                    Some(printer_operation::Operation::NozzleHolderCtrl(value))
                        if value.action == 2
                ));
            }) as fn(&Option<printer_operation::Operation>),
        ),
        (
            PrinterOperationKind::NozzleInfoConfirm { id: 0xff },
            (|operation: &Option<printer_operation::Operation>| {
                assert!(matches!(
                    operation,
                    Some(printer_operation::Operation::NozzleInfoConfirm(value))
                        if value.id == 0xff
                ));
            }) as fn(&Option<printer_operation::Operation>),
        ),
        (
            PrinterOperationKind::HolderNozzleRefresh { id: 17 },
            (|operation: &Option<printer_operation::Operation>| {
                assert!(matches!(
                    operation,
                    Some(printer_operation::Operation::HolderNozzleRefresh(value))
                        if value.id == 17
                ));
            }) as fn(&Option<printer_operation::Operation>),
        ),
    ] {
        let payload = PrinterOperationPayload {
            printer_id: "printer-1".to_string(),
            serial_number: "SERIAL123".to_string(),
            operation,
        };
        let command = CommandRecord::from_parts(CommandRecordParts {
            id: CommandId::new(),
            tenant_id: TenantId::new(),
            agent_id: AgentId::new(),
            printer_id: Some("printer-1".to_string()),
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
                assert_operation(&command.operation);
            }
            other => panic!("expected printer operation command, got {other:?}"),
        }
    }
}
