use tokio::sync::mpsc;

use super::*;

#[tokio::test]
async fn h2c_auto_mapping_returns_correlated_failure_from_failed_command() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-h2c-mapping", "Plugin H2C Mapping")
        .await
        .unwrap();
    let auth_token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "h2c-mapping").await;
    let agent_id = feature_advertisement_printer_with_model(
        &state,
        tenant.id,
        "h2c-mapping-agent",
        "H2C-MAPPING",
        "O1C2",
    )
    .await;
    let session = crate::sessions::SessionToken::new();
    claim_feature_session(&state, tenant.id, agent_id, session).await;
    let (command_sender, mut command_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: session,
            tenant_id: tenant.id,
            agent_id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: "2026-08-01T00:00:00Z".to_owned(),
            last_heartbeat_at: "2026-08-01T00:00:00Z".to_owned(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender,
            capabilities: [
                pandar_protocol::agent::v1::AgentCapability::RequiredDeviceFeatures,
                pandar_protocol::agent::v1::AgentCapability::H2cAutoNozzleMapping,
            ]
            .into_iter()
            .collect(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    let printer = state
        .printers()
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent_id,
            &session.persisted_id(),
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "H2C-MAPPING".to_owned(),
                host: None,
                access_code: None,
                name: "H2C Mapping".to_owned(),
                model: Some("O1C2".to_owned()),
                status: Some("idle".to_owned()),
                observed_at: "2026-08-01T00:00:00Z".to_owned(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: Some(
                    serde_json::from_value(serde_json::json!({
                        "nozzle": {
                            "exist": 65536,
                            "state": 0,
                            "info": [{"id": 16, "diameter": 0.4, "type": "XS01", "stat": 0}]
                        },
                        "holder": {"stat": 0, "pos": 2, "info": 0}
                    }))
                    .unwrap(),
                ),
                connection_authoritative: false,
                telemetry_authoritative: false,
            },
            Some(pandar_core::BambuDeviceFeatures::from_bits(1_u64 << 60)),
        )
        .await
        .unwrap();

    let uri = format!("/api/v1/plugin/printers/{}/auto-nozzle-mapping", printer.id);
    let request_app = app.clone();
    let request_token = auth_token.clone();
    let request_task = tokio::spawn(async move {
        request_as(
            request_app,
            Method::POST,
            &uri,
            Some(serde_json::json!({
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "42",
                "version": 1,
                "group_info": [{
                    "id": 0,
                    "ext": 1,
                    "dia": 0.4,
                    "vol": "E3D High Flow"
                }]
            })),
            &request_token,
        )
        .await
    });
    let emitted = command_receiver.recv().await.unwrap().unwrap();
    let command_id = pandar_core::CommandId::parse(&emitted.command_id).unwrap();
    let Some(pandar_protocol::agent::v1::hub_command::Command::PrinterOperation(operation)) =
        emitted.command
    else {
        panic!("expected printer operation");
    };
    let Some(pandar_protocol::agent::v1::printer_operation::Operation::GetAutoNozzleMapping(
        operation,
    )) = operation.operation
    else {
        panic!("expected H2C auto nozzle mapping operation");
    };
    assert_eq!(operation.sequence_id, "42");
    assert_eq!(operation.version, Some(1));

    state
        .commands()
        .mark_failed_with_result(
            command_id,
            tenant.id,
            agent_id,
            "rack busy",
            Some(
                serde_json::json!({
                    "type": "printer_operation",
                    "action": "get_auto_nozzle_mapping",
                    "mqtt_report": {
                        "print": {
                            "command": "get_auto_nozzle_mapping",
                            "sequence_id": "42",
                            "result": "fail",
                            "version": "future",
                            "reason": "rack busy",
                            "errno": 17
                        }
                    }
                })
                .to_string(),
            ),
        )
        .await
        .unwrap();

    let (status, body) = request_task.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["print"]["result"], "fail");
    assert_eq!(body["print"]["reason"], "rack busy");
    assert_eq!(body["print"]["errno"], 17);

    let timeout_uri = format!("/api/v1/plugin/printers/{}/auto-nozzle-mapping", printer.id);
    let timeout_task = tokio::spawn(async move {
        request_as(
            app,
            Method::POST,
            &timeout_uri,
            Some(serde_json::json!({
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "43",
                "version": 1,
                "group_info": [{
                    "id": 0,
                    "ext": 1,
                    "dia": 0.4,
                    "vol": "High Flow"
                }]
            })),
            &auth_token,
        )
        .await
    });
    let timed_out = command_receiver.recv().await.unwrap().unwrap();
    let timed_out_id = pandar_core::CommandId::parse(&timed_out.command_id).unwrap();
    let (status, body) = timeout_task.await.unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "h2c_auto_nozzle_mapping_unavailable");
    let persisted = state
        .commands()
        .get_for_tenant(tenant.id, timed_out_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, pandar_core::CommandStatus::Failed);
    assert!(
        persisted
            .error
            .as_deref()
            .unwrap()
            .contains("response timed out")
    );

    register_capability_session(
        &state,
        tenant.id,
        agent_id,
        [pandar_protocol::agent::v1::AgentCapability::H2cAutoNozzleMapping],
    )
    .await;
    let printer = state
        .printers()
        .get_for_tenant(tenant.id, &printer.id)
        .await
        .unwrap()
        .unwrap();
    let request = serde_json::from_value(serde_json::json!({
        "command": "get_auto_nozzle_mapping",
        "sequence_id": "44",
        "version": 1,
        "group_info": [{"id": 0, "ext": 1, "dia": 0.4, "vol": "Standard"}]
    }))
    .unwrap();
    let stale_dispatch = crate::routes::printer_operations::live::dispatch_for_printer_with_token(
        &state,
        tenant.id,
        printer,
        crate::repositories::PrinterOperationKind::GetAutoNozzleMapping { request },
        crate::repositories::AuditActor::tenant_token(None, "h2c-test", vec!["plugin:studio"]),
        session,
        pandar_protocol::agent::v1::AgentCapability::H2cAutoNozzleMapping,
    )
    .await;
    assert!(stale_dispatch.is_err());
}
