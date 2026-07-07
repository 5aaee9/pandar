use super::*;
use requests::{
    plugin_login_ticket_body, plugin_ticket_exchange_body, redacted_audit_metadata_fixture,
    safe_audit_metadata_fixture,
};
use serde::{Deserialize, Serialize, de::IgnoredAny};

mod requests;

#[derive(Debug, Deserialize)]
struct LoginTicketResponse {
    ticket: String,
    expires_at: String,
    redirect_url: String,
}

#[derive(Debug, Deserialize)]
struct ExchangeLoginTicketResponse {
    token: String,
    expires_at: String,
    profile: PluginProfileResponse,
}

#[derive(Debug, Deserialize)]
struct PluginProfileResponse {
    tenant_id: String,
    tenant_name: String,
}

#[derive(Debug, Deserialize)]
struct PluginPrinterListResponse {
    message: String,
    devices: Vec<PluginPrinterResponse>,
    #[serde(default)]
    printers: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PluginPrinterResponse {
    dev_id: String,
    dev_name: String,
    name: String,
    dev_ip: Option<String>,
    dev_access_code: Option<String>,
    dev_model_name: Option<String>,
    model: Option<String>,
    dev_online: bool,
    online: bool,
    task_status: String,
    state: String,
    pandar_printer_id: String,
    nozzle_temperatures: Vec<PluginNozzleTemperatureResponse>,
    active_nozzle: Option<String>,
    bed_temperature_celsius: Option<String>,
    bed_target_temperature_celsius: Option<String>,
    chamber_temperature_celsius: Option<String>,
    chamber_light_on: Option<bool>,
    materials: PluginMaterialsResponse,
}

#[derive(Debug, Deserialize)]
struct PluginNozzleTemperatureResponse {
    current_celsius: Option<String>,
    target_celsius: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginMaterialsResponse {
    ams_units: Vec<PluginAmsUnitResponse>,
    external_spools: Vec<PluginExternalSpoolResponse>,
    active_tray: PluginActiveTrayResponse,
}

#[derive(Debug, Deserialize)]
struct PluginAmsUnitResponse {
    unit_id: String,
}

#[derive(Debug, Deserialize)]
struct PluginExternalSpoolResponse {
    external_id: String,
}

#[derive(Debug, Deserialize)]
struct PluginActiveTrayResponse {
    global_tray_id: u32,
}

#[derive(Debug, Deserialize)]
struct PluginPrintResponse {
    task_id: String,
    command_id: String,
    status: String,
    message: Option<String>,
    artifact_metadata: Option<SlicerMetadataResponse>,
    pandar_job_id: String,
    #[serde(default)]
    print: Option<Value>,
    #[serde(default)]
    artifact: Option<Value>,
    #[serde(default)]
    printer_id: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PluginJobListResponse {
    jobs: Vec<PluginJobResponse>,
}

#[derive(Debug, Deserialize)]
struct PluginJobResponse {
    artifact_metadata: Option<SlicerMetadataResponse>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct SlicerMetadataResponse {
    display_name: String,
    default_plate_id: i64,
}

#[derive(Debug, Deserialize)]
struct PluginTokenAuditMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
    token: Option<String>,
    ticket: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuditEventListResponse<T> {
    audit_events: Vec<AuditEventResponse<T>>,
}

#[derive(Debug, Deserialize)]
struct AuditEventResponse<T> {
    action: String,
    metadata: T,
}

#[derive(Debug, Deserialize)]
struct RedactedAuditMetadata {
    safe: String,
    nested: RedactedNestedAuditMetadata,
    subject: Option<String>,
    plaintext_token: Option<String>,
    ticket: Option<String>,
    plaintext_ticket: Option<String>,
    headers: RedactedHeaders,
    artifact_storage_path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RedactedNestedAuditMetadata {
    ok: bool,
}

#[derive(Debug, Deserialize)]
struct RedactedHeaders {
    #[serde(rename = "Authorization")]
    authorization: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyAuditMetadata {}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchFixture {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
    ams_units: [PluginMaterialPatchAmsUnit; 1],
    external_spools: [PluginMaterialPatchExternalSpool; 1],
    active_tray: PluginMaterialPatchActiveTray,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchAmsUnit {
    unit_id: &'static str,
    humidity: u8,
    humidity_level: u8,
    temperature_celsius: f64,
    toolhead: &'static str,
    trays: [PluginMaterialPatchTray; 1],
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchTray {
    tray_id: &'static str,
    global_tray_id: u8,
    #[serde(rename = "type")]
    material_type: &'static str,
    filament_id: &'static str,
    color: &'static str,
    remaining_estimate: &'static str,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchExternalSpool {
    external_id: &'static str,
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    filament_id: &'static str,
    color: &'static str,
    toolhead: &'static str,
}

#[derive(Debug, Serialize)]
struct PluginMaterialPatchActiveTray {
    kind: &'static str,
    ams_id: &'static str,
    tray_id: &'static str,
    global_tray_id: u8,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

#[tokio::test]
async fn plugin_login_ticket_creation_enforces_external_viewer_or_all_tenant_token() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("plugin-acme", "Plugin Acme")
        .await
        .unwrap();
    let viewer = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "plugin-viewer",
    )
    .await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "plugin-all").await;
    let empty = read_only_tenant_token(&state, &tenant.id.to_string(), "plugin-empty").await;
    let agent_register =
        agent_register_tenant_token(&state, &tenant.id.to_string(), "plugin-agent").await;
    let plugin_studio =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "plugin-studio").await;
    let uri = format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id);
    let body = || plugin_login_ticket_body("http://localhost:4100/callback?state=abc");

    let (status, viewer_body) = request_as(app.clone(), Method::POST, &uri, body(), &viewer).await;
    assert_eq!(status, StatusCode::CREATED);
    let viewer_body = decode::<LoginTicketResponse>(viewer_body);
    assert!(viewer_body.ticket.starts_with("pandar_plugin_ticket_"));
    assert!(viewer_body.expires_at.ends_with('Z'));
    assert_eq!(
        viewer_body.redirect_url,
        "http://localhost:4100/callback?state=abc"
    );

    let (status, _) = request_as(app.clone(), Method::POST, &uri, body(), &all).await;
    assert_eq!(status, StatusCode::CREATED);
    for denied in [&empty, &agent_register, &plugin_studio] {
        let (status, body) = request_as(app.clone(), Method::POST, &uri, body(), denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    }

    for redirect_url in [
        "https://localhost:4100/callback",
        "http://example.test:4100/callback",
        "http://localhost/callback",
        "http://user:pass@localhost:4100/callback",
        "http://localhost:4100/callback#fragment",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &uri,
            plugin_login_ticket_body(redirect_url),
            &viewer,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_redirect_url");
    }
}

#[tokio::test]
async fn plugin_login_ticket_exchange_is_unauthenticated_one_use_and_rejects_expired() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("plugin-exchange", "Plugin Exchange")
        .await
        .unwrap();
    let viewer = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "plugin-exchange-viewer",
    )
    .await;
    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id),
        plugin_login_ticket_body("http://127.0.0.1:4100/callback"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket = decode::<LoginTicketResponse>(created).ticket;

    let (status, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let exchanged = decode::<ExchangeLoginTicketResponse>(exchanged);
    assert!(exchanged.token.starts_with("pandar_plugin_"));
    assert!(exchanged.expires_at.ends_with('Z'));
    assert_eq!(exchanged.profile.tenant_id, tenant.id.to_string());
    assert_eq!(exchanged.profile.tenant_name, "Plugin Exchange");

    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");

    let expired = state
        .auth()
        .create_plugin_login_ticket_with_audit(
            tenant.id,
            None,
            "http://localhost:4100/expired",
            "2026-01-01T00:00:00Z".to_owned(),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    sqlx::query("UPDATE plugin_login_tickets SET expires_at = ?2 WHERE id = ?1")
        .bind(&expired.ticket.id)
        .bind("2026-01-01T00:00:00Z")
        .execute(sqlite_pool(&state))
        .await
        .unwrap();
    let (status, body) = request(
        app,
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&expired.plaintext_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");
}

#[tokio::test]
async fn plugin_no_auth_session_is_only_available_in_no_auth_mode() {
    let no_auth_state = state().await.with_no_auth_for_tests(true);
    let app = router(no_auth_state.clone());
    let tenant = no_auth_state
        .tenants()
        .create("plugin-no-auth", "Plugin No Auth")
        .await
        .unwrap();

    let (status, session) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/no-auth-session",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let session = decode::<ExchangeLoginTicketResponse>(session);
    assert!(session.token.starts_with("pandar_tenant_"));
    assert_eq!(session.profile.tenant_id, tenant.id.to_string());
    assert_eq!(session.profile.tenant_name, "Plugin No Auth");

    let (status, body) = request_as(
        app,
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &session.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert!(body.devices.is_empty());

    let app = router(state().await);
    let (status, body) = request(app, Method::POST, "/api/v1/plugin/no-auth-session", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "no_auth_required");
}

#[tokio::test]
async fn plugin_routes_only_accept_plugin_studio_tokens() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-auth", "Plugin Auth")
        .await
        .unwrap();
    let plugin = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "studio").await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "all").await;
    let empty = read_only_tenant_token(&state, &tenant.id.to_string(), "empty").await;
    let mixed = all_and_plugin_studio_tenant_token(&state, &tenant.id.to_string(), "mixed").await;

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &plugin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert!(body.devices.is_empty());

    for denied in [&all, &empty, &mixed] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            "/api/v1/plugin/printers",
            None,
            denied,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    }
}

#[tokio::test]
async fn plugin_printer_list_returns_studio_devices_shape() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-device-list", "Plugin Device List")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "devices").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer = state
        .printers()
        .upsert_snapshot(
            tenant.id,
            agent.id,
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "studio-printer-1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("studio-access-code".to_string()),
                name: "Studio Printer".to_string(),
                model: Some("Bambu Lab X2D".to_string()),
                status: "IDLE".to_string(),
                observed_at: "2026-06-20T00:00:00Z".to_string(),
                nozzle_temperatures: vec![
                    pandar_core::PrinterNozzleTemperature {
                        label: Some("L".to_string()),
                        current_celsius: Some("28".to_string()),
                        target_celsius: Some("220".to_string()),
                        diameter_mm: None,
                        nozzle_type: None,
                    },
                    pandar_core::PrinterNozzleTemperature {
                        label: Some("R".to_string()),
                        current_celsius: Some("27".to_string()),
                        target_celsius: Some("215".to_string()),
                        diameter_mm: None,
                        nozzle_type: None,
                    },
                ],
                active_nozzle: Some("L".to_string()),
                bed_temperature_celsius: Some("60".to_string()),
                bed_target_temperature_celsius: Some("65".to_string()),
                chamber_temperature_celsius: Some("32".to_string()),
                chamber_light_on: Some(true),
            },
        )
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id: printer.id.clone(),
            serial_number: "studio-printer-1".to_string(),
            printer_materials_json: serde_json::to_string(&PluginMaterialPatchFixture {
                kind: "printer_material_patch",
                observed_at: "2026-06-20T00:01:00Z",
                ams_units: [PluginMaterialPatchAmsUnit {
                    unit_id: "0",
                    humidity: 25,
                    humidity_level: 3,
                    temperature_celsius: 28.5,
                    toolhead: "R",
                    trays: [PluginMaterialPatchTray {
                        tray_id: "0",
                        global_tray_id: 0,
                        material_type: "PLA",
                        filament_id: "GFL99",
                        color: "00FF00",
                        remaining_estimate: "72",
                    }],
                }],
                external_spools: [PluginMaterialPatchExternalSpool {
                    external_id: "254",
                    tray_id: "0",
                    material_type: "PETG",
                    filament_id: "GFG00",
                    color: "11223344",
                    toolhead: "L",
                }],
                active_tray: PluginMaterialPatchActiveTray {
                    kind: "ams",
                    ams_id: "0",
                    tray_id: "0",
                    global_tray_id: 0,
                },
            })
            .unwrap(),
        })
        .await
        .unwrap();

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert_eq!(body.printers, None);
    assert_eq!(body.devices.len(), 1);
    let device = &body.devices[0];
    assert_eq!(device.dev_id, "studio-printer-1");
    assert_eq!(device.dev_name, "Studio Printer");
    assert_eq!(device.name, "Studio Printer");
    assert_eq!(device.dev_ip.as_deref(), Some("192.0.2.10"));
    assert_eq!(
        device.dev_access_code.as_deref(),
        Some("studio-access-code")
    );
    assert_eq!(device.dev_model_name.as_deref(), Some("N6"));
    assert_eq!(device.model.as_deref(), Some("Bambu Lab X2D"));
    assert!(device.dev_online);
    assert!(device.online);
    assert_eq!(device.task_status, "IDLE");
    assert_eq!(device.state, "IDLE");
    assert_eq!(device.pandar_printer_id, printer.id);
    assert_eq!(
        device.nozzle_temperatures[0].current_celsius.as_deref(),
        Some("28")
    );
    assert_eq!(
        device.nozzle_temperatures[0].target_celsius.as_deref(),
        Some("220")
    );
    assert_eq!(
        device.nozzle_temperatures[1].current_celsius.as_deref(),
        Some("27")
    );
    assert_eq!(
        device.nozzle_temperatures[1].target_celsius.as_deref(),
        Some("215")
    );
    assert_eq!(device.active_nozzle.as_deref(), Some("L"));
    assert_eq!(device.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(device.bed_target_temperature_celsius.as_deref(), Some("65"));
    assert_eq!(device.chamber_temperature_celsius.as_deref(), Some("32"));
    assert_eq!(device.chamber_light_on, Some(true));
    assert_eq!(device.materials.ams_units[0].unit_id, "0");
    assert_eq!(device.materials.external_spools[0].external_id, "254");
    assert_eq!(device.materials.active_tray.global_tray_id, 0);
}

#[tokio::test]
async fn plugin_print_returns_job_shape_and_records_plugin_actor_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print", "Plugin Print")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<PluginPrintResponse>(body);
    assert_eq!(body.status, "queued");
    assert_eq!(body.message, None);
    assert_eq!(body.artifact_metadata, None);
    assert!(!body.task_id.is_empty());
    assert!(!body.command_id.is_empty());
    assert_eq!(body.pandar_job_id, body.task_id);
    assert_eq!(body.print, None);
    assert_eq!(body.artifact, None);
    assert_eq!(body.printer_id, None);

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.create")
        .unwrap();
    let metadata: PluginTokenAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(event.actor_type, "plugin_token");
    assert!(!metadata.tenant_token_id.is_empty());
    assert_eq!(
        metadata.tenant_token_scopes,
        vec!["plugin:studio".to_owned()]
    );
    assert_eq!(metadata.token, None);
    assert_eq!(metadata.ticket, None);
}

#[tokio::test]
async fn plugin_print_handles_concurrent_sqlite_writes() {
    let state = AppState::file_sqlite_for_tests()
        .await
        .unwrap()
        .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-concurrent", "Plugin Print Concurrent")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let mut requests = Vec::new();
    for index in 0..2 {
        let token = plugin_studio_tenant_token(
            &state,
            &tenant.id.to_string(),
            &format!("print-plugin-{index}"),
        )
        .await;
        let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
            .await
            .unwrap();
        requests.push((token, printer_id));
    }

    let responses =
        futures_util::future::join_all(requests.into_iter().map(|(token, printer_id)| {
            let app = app.clone();
            async move {
                multipart_request_as(
                    app,
                    Method::POST,
                    "/api/v1/plugin/prints",
                    multipart_print_body(
                        Some(&printer_id),
                        Some(("plugin concurrent.3mf", "model/3mf", b"abc")),
                        1,
                    ),
                    &token,
                )
                .await
            }
        }))
        .await;

    for (status, body) in responses {
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let body = decode::<PluginPrintResponse>(body);
        assert_eq!(body.status, "queued");
        assert!(!body.task_id.is_empty());
        assert!(!body.command_id.is_empty());
    }
}

#[tokio::test]
async fn plugin_print_and_list_include_artifact_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-metadata", "Plugin Print Metadata")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-metadata-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", &artifact)),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<PluginPrintResponse>(body);
    let artifact_metadata = body.artifact_metadata.unwrap();
    assert_eq!(artifact_metadata.display_name, "plate file");
    assert_eq!(artifact_metadata.default_plate_id, 1);

    let (status, list) = request_as(app, Method::GET, "/api/v1/plugin/jobs", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    let list = decode::<PluginJobListResponse>(list);
    assert_eq!(list.jobs[0].artifact_metadata, Some(artifact_metadata));
}

#[tokio::test]
async fn plugin_print_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-sibling", "Plugin Print Sibling")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "sibling-print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(decode::<PluginPrintResponse>(body).status, "queued");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn audit_events_route_authorizes_paginates_filters_and_redacts_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-plugin", "Audit Plugin")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "audit-admin",
    )
    .await;
    let viewer = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "audit-viewer",
    )
    .await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "audit-all").await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "first.action",
        "2026-06-20T00:00:00Z",
        redacted_audit_metadata_fixture(),
    )
    .await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "second.action",
        "2026-06-21T00:00:00Z",
        safe_audit_metadata_fixture("second"),
    )
    .await;

    let uri = format!("/api/v1/tenants/{}/audit-events?limit=1", tenant.id);
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &admin).await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<IgnoredAny>>(body);
    assert_eq!(body.audit_events.len(), 1);
    assert_eq!(body.audit_events[0].action, "second.action");

    let uri = format!(
        "/api/v1/tenants/{}/audit-events?before=2026-06-21T00:00:00Z&action=first.action",
        tenant.id
    );
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &all).await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<RedactedAuditMetadata>>(body);
    let metadata = &body.audit_events[0].metadata;
    assert_eq!(metadata.safe, "keep");
    assert_eq!(metadata.nested, RedactedNestedAuditMetadata { ok: true });
    assert_eq!(metadata.subject, None);
    assert_eq!(metadata.plaintext_token, None);
    assert_eq!(metadata.ticket, None);
    assert_eq!(metadata.plaintext_ticket, None);
    assert_eq!(metadata.headers.authorization, None);
    assert_eq!(metadata.artifact_storage_path, None);

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events?limit=0", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_limit");

    let (status, body) = request_as(app, Method::GET, &uri, None, &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn audit_events_route_falls_back_to_empty_metadata_for_invalid_persisted_json() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-invalid", "Audit Invalid")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "invalid-audit-admin",
    )
    .await;
    insert_raw_audit_fixture(
        &state,
        tenant.id,
        "invalid.metadata",
        "2026-06-20T00:00:00Z",
        "{not-json",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events", tenant.id),
        None,
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<EmptyAuditMetadata>>(body);
    assert_eq!(body.audit_events.len(), 1);
}

async fn insert_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata: Value,
) {
    insert_raw_audit_fixture(state, tenant_id, action, created_at, &metadata.to_string()).await;
}

async fn insert_raw_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata_json: &str,
) {
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at)
         VALUES (?1, ?2, 'user', NULL, ?3, 'fixture', NULL, ?4, ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(tenant_id.to_string())
    .bind(action)
    .bind(metadata_json)
    .bind(created_at)
    .execute(sqlite_pool(state))
    .await
    .unwrap();
}

fn sqlite_pool(state: &AppState) -> &sqlx::SqlitePool {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    pool
}
