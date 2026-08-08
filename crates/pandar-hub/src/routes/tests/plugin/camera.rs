use tokio::sync::mpsc;

use super::*;
use crate::protocol::agent::v1::{
    AgentCapability, CameraStreamMode, HubCommand, hub_camera_command, hub_command,
};

#[tokio::test]
async fn plugin_local_camera_opens_mjpeg_tunnel_for_capable_agent() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-local-camera", "Plugin Local Camera")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "camera").await;
    let agent = state
        .agents()
        .create(tenant.id, "camera-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant.id,
        agent.id,
        Some("Bambu Lab A1 Mini"),
    )
    .await
    .unwrap();
    let (command_sender, mut command_receiver) = mpsc::channel(2);
    let session = register_camera_session(&state, tenant.id, agent.id, command_sender, true).await;
    state
        .printers()
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            &session.persisted_id(),
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "SERIAL-1".to_owned(),
                host: None,
                access_code: None,
                name: "A1 Mini".to_owned(),
                model: Some("Bambu Lab A1 Mini".to_owned()),
                status: Some("idle".to_owned()),
                observed_at: pandar_core::created_at_now(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
            None,
        )
        .await
        .unwrap();

    let (list_status, list_body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let camera_printer = decode::<PluginPrinterListResponse>(list_body)
        .devices
        .into_iter()
        .find(|printer| printer.dev_id == "SERIAL-1")
        .unwrap();
    assert!(camera_printer.studio_local_camera);

    let response = raw_request_as(
        app,
        Method::GET,
        &format!("/api/v1/plugin/printers/{printer_id}/camera.mjpeg"),
        &token,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "multipart/x-mixed-replace; boundary=frame"
    );
    let command = command_receiver.recv().await.unwrap().unwrap();
    match command.command.unwrap() {
        hub_command::Command::CameraStream(command) => match command.command.unwrap() {
            hub_camera_command::Command::Open(open) => {
                assert_eq!(open.mode, CameraStreamMode::Mjpeg as i32);
            }
            other => panic!("expected camera open command, got {other:?}"),
        },
        other => panic!("expected camera stream command, got {other:?}"),
    }
}

#[tokio::test]
async fn plugin_local_camera_rejects_unverified_models_and_incapable_agents() {
    for (slug, model, capable) in [
        ("p1p", "Bambu Lab P1P", true),
        ("x1c", "Bambu Lab X1 Carbon", true),
        ("unknown", "Future Printer", true),
        ("old-agent", "Bambu Lab A1", false),
    ] {
        let state = state().await;
        let app = router(state.clone());
        let tenant = state
            .tenants()
            .create(&format!("camera-{slug}"), &format!("Camera {slug}"))
            .await
            .unwrap();
        let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), slug).await;
        let agent = state.agents().create(tenant.id, slug).await.unwrap();
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
            state.database(),
            tenant.id,
            agent.id,
            Some(model),
        )
        .await
        .unwrap();
        register_camera_session(&state, tenant.id, agent.id, mpsc::channel(1).0, capable).await;

        let (status, body) = request_as(
            app,
            Method::GET,
            &format!("/api/v1/plugin/printers/{printer_id}/camera.mjpeg"),
            None,
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{slug}");
        assert_eq!(decode::<ErrorResponse>(body).error, "camera_unavailable");
    }
}

async fn register_camera_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    command_sender: mpsc::Sender<Result<HubCommand, tonic::Status>>,
    capable: bool,
) -> crate::sessions::SessionToken {
    let token = crate::sessions::SessionToken::new();
    claim_feature_session(state, tenant_id, agent_id, token).await;
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "camera-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender,
            capabilities: capable
                .then_some(AgentCapability::StudioLocalCamera)
                .into_iter()
                .collect(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    token
}
