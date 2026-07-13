use std::{collections::HashSet, sync::Arc};

use axum::http::{Method, StatusCode};
use pandar_core::{FirmwareCatalogEntry, PrinterFirmwareState, PrinterUpgradeState};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use super::super::*;
use super::support::*;
use crate::{
    protocol::agent::v1::AgentCapability,
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};

#[tokio::test]
async fn plugin_firmware_routes_require_only_plugin_studio_auth_and_hide_other_tenants() {
    let fixture = FirmwareRouteFixture::new("plugin-firmware-auth").await;
    let uri = fixture.uri("");

    let (status, body) = request(fixture.app(), Method::GET, &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "missing_auth_token");

    let all = all_scope_tenant_token(
        &fixture.state,
        &fixture.tenant_id.to_string(),
        "firmware-all-scope",
    )
    .await;
    let (status, body) = request_as(fixture.app(), Method::GET, &uri, None, &all).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "role_forbidden");

    let other = fixture
        .state
        .tenants()
        .create("plugin-firmware-other", "Other")
        .await
        .unwrap();
    let other_auth =
        plugin_studio_tenant_token(&fixture.state, &other.id.to_string(), "firmware-other").await;
    let (status, body) = request_as(fixture.app(), Method::GET, &uri, None, &other_auth).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "printer_not_found");

    let missing = "/api/v1/plugin/printers/missing/firmware";
    let (status, body) = request_as(fixture.app(), Method::GET, missing, None, &fixture.auth).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "printer_not_found");
}

#[tokio::test]
async fn plugin_firmware_state_and_batch_share_exact_current_projection_and_empty_typed_catalog() {
    let fixture = FirmwareRouteFixture::new("plugin-firmware-projection").await;
    let (status, initial) = request_as(
        fixture.app(),
        Method::GET,
        &fixture.uri(""),
        None,
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let initial = serde_json::from_value::<TypedFirmwareStateResponse>(initial).unwrap();
    assert_eq!(initial.catalog, Vec::<FirmwareCatalogEntry>::new());
    assert_eq!(
        initial.firmware.session_id,
        Some(fixture.token.persisted_id())
    );
    assert_eq!(initial.firmware.generation, Some(GENERATION));
    assert_eq!(initial.firmware.modules, None);
    assert_eq!(initial.firmware.upgrade_state, None);
    assert_eq!(initial.firmware.cfg, None);
    fixture
        .state
        .printers()
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            3,
            vec![module("future/unit", "09.08.07")],
        )
        .await
        .unwrap();
    fixture
        .state
        .printers()
        .replace_status_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            5,
            Some(PrinterUpgradeState {
                status: Some("IDLE".to_owned()),
                progress: Some("0".to_owned()),
                message: None,
                module: None,
                error_code: Some(0),
                new_version_state: None,
                consistency_request: Some(false),
                force_upgrade: Some(false),
                display_state: None,
                ota_new_version_number: None,
                ams_new_version_number: None,
                ahb_new_version_number: None,
                new_versions: Some(Vec::new()),
                ams_firmware: None,
            }),
            Some("4".to_owned()),
        )
        .await
        .unwrap();

    let (status, direct) = request_as(
        fixture.app(),
        Method::GET,
        &fixture.uri(""),
        None,
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let typed = serde_json::from_value::<TypedFirmwareStateResponse>(direct.clone()).unwrap();
    assert_eq!(typed.catalog, Vec::<FirmwareCatalogEntry>::new());
    assert_eq!(typed.firmware.module_revision, 3);
    assert_eq!(typed.firmware.status_revision, 5);
    assert_eq!(direct["catalog"], json!([]));
    assert_eq!(
        direct["firmware"]["session_id"],
        fixture.token.persisted_id()
    );
    assert_eq!(direct["firmware"]["generation"], GENERATION);
    assert_eq!(direct["firmware"]["module_revision"], 3);
    assert_eq!(direct["firmware"]["status_revision"], 5);

    let (status, batch) = request_as(
        fixture.app(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(batch["devices"][0]["firmware"], direct["firmware"]);
}

#[tokio::test]
async fn plugin_firmware_projection_rejects_incapable_stale_noncurrent_wrong_owner_and_replica() {
    let incapable = FirmwareRouteFixture::with_capability("plugin-firmware-incapable", false).await;
    assert_unavailable(&incapable, incapable.app()).await;

    let stale = FirmwareRouteFixture::new("plugin-firmware-stale").await;
    let replacement = SessionToken::new();
    stale
        .state
        .agents()
        .claim_online_session(
            stale.tenant_id,
            stale.agent_id,
            &replacement.persisted_id(),
            "replacement",
            "2026-07-12T00:00:01Z",
        )
        .await
        .unwrap();
    stale
        .state
        .sessions()
        .register(AgentSession {
            token: replacement,
            tenant_id: stale.tenant_id,
            agent_id: stale.agent_id,
            name: "replacement".to_owned(),
            version: "test".to_owned(),
            connected_at: "2026-07-12T00:00:01Z".to_owned(),
            last_heartbeat_at: "2026-07-12T00:00:01Z".to_owned(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender: mpsc::channel(1).0,
            capabilities: HashSet::from([AgentCapability::FirmwareControl]),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    assert_unavailable(&stale, stale.app()).await;

    let replica = FirmwareRouteFixture::new("plugin-firmware-replica").await;
    assert_unavailable(&replica, router(sibling_state(&replica.state))).await;

    let wrong_owner = FirmwareRouteFixture::new("plugin-firmware-wrong-owner").await;
    let other_agent = wrong_owner
        .state
        .agents()
        .create(wrong_owner.tenant_id, "other-owner")
        .await
        .unwrap();
    sqlx::query("UPDATE printers SET agent_id = ?1 WHERE id = ?2")
        .bind(other_agent.id.to_string())
        .bind(&wrong_owner.printer_id)
        .execute(sqlite_pool(&wrong_owner.state))
        .await
        .unwrap();
    assert_unavailable(&wrong_owner, wrong_owner.app()).await;
}

async fn assert_unavailable(fixture: &FirmwareRouteFixture, app: axum::Router) {
    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &fixture.uri(""),
        None,
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "firmware_control_unavailable");
    let (status, batch) = request_as(
        app,
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let device = batch["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|device| device["pandar_printer_id"] == fixture.printer_id)
        .unwrap();
    assert!(device.get("firmware").is_none());
}

#[derive(Debug, Deserialize)]
struct TypedFirmwareStateResponse {
    firmware: PrinterFirmwareState,
    catalog: Vec<FirmwareCatalogEntry>,
}
