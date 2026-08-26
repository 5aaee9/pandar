use std::collections::{BTreeMap, HashSet};

use pandar_core::PrinterFirmwareState;

use super::*;
use crate::sessions::{AgentSession, SessionToken, empty_pending_live_commands};
use pandar_protocol::agent::v1::AgentCapability;

#[tokio::test]
async fn plugin_firmware_batch_requires_current_capable_exact_session_generation() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-firmware-batch", "Plugin Firmware Batch")
        .await
        .unwrap();
    let auth = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "firmware-batch").await;

    let matching = firmware_printer(&state, tenant.id, "matching", "FW-MATCHING").await;
    let matching_token = register_firmware_session(&state, tenant.id, matching, true).await;
    establish(
        &state,
        tenant.id,
        matching,
        matching_token,
        "FW-MATCHING",
        3,
    )
    .await;
    state
        .printers()
        .replace_modules_if_current(
            tenant.id,
            matching,
            &matching_token.persisted_id(),
            "FW-MATCHING",
            3,
            1,
            Vec::new(),
        )
        .await
        .unwrap();

    let incapable = firmware_printer(&state, tenant.id, "incapable", "FW-INCAPABLE").await;
    let incapable_token = register_firmware_session(&state, tenant.id, incapable, false).await;
    establish(
        &state,
        tenant.id,
        incapable,
        incapable_token,
        "FW-INCAPABLE",
        4,
    )
    .await;

    let replaced = firmware_printer(&state, tenant.id, "replaced", "FW-REPLACED").await;
    let old_token = register_firmware_session(&state, tenant.id, replaced, true).await;
    establish(&state, tenant.id, replaced, old_token, "FW-REPLACED", 5).await;
    register_firmware_session(&state, tenant.id, replaced, true).await;

    let absent = firmware_printer(&state, tenant.id, "absent", "FW-ABSENT").await;
    register_firmware_session(&state, tenant.id, absent, true).await;

    let (status, body) = request_as(app, Method::GET, "/api/v1/plugin/printers", None, &auth).await;
    assert_eq!(status, StatusCode::OK);
    let firmware = decode::<PluginPrinterListResponse>(body)
        .devices
        .into_iter()
        .map(|device| (device.dev_id, device.firmware))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        firmware["FW-MATCHING"],
        Some(PrinterFirmwareState {
            session_id: Some(matching_token.persisted_id()),
            generation: Some(3),
            module_revision: 1,
            status_revision: 0,
            modules: Some(Vec::new()),
            upgrade_state: None,
            cfg: None,
        })
    );
    assert_eq!(firmware["FW-INCAPABLE"], None);
    assert_eq!(firmware["FW-REPLACED"], None);
    assert_eq!(firmware["FW-ABSENT"], None);
}

async fn firmware_printer(
    state: &AppState,
    tenant_id: TenantId,
    agent_name: &str,
    serial: &str,
) -> pandar_core::AgentId {
    feature_advertisement_printer(state, tenant_id, agent_name, serial).await
}

async fn register_firmware_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    capable: bool,
) -> SessionToken {
    let token = SessionToken::new();
    state
        .agents()
        .claim_online_session(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            "test",
            "2026-07-12T00:00:00Z",
        )
        .await
        .unwrap();
    let capabilities = capable
        .then_some(AgentCapability::FirmwareControl)
        .into_iter()
        .collect::<HashSet<_>>();
    state
        .sessions()
        .register(AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: "2026-07-12T00:00:00Z".to_owned(),
            last_heartbeat_at: "2026-07-12T00:00:00Z".to_owned(),
            wake_sender: tokio::sync::mpsc::channel(1).0,
            close_sender: tokio::sync::mpsc::channel(1).0,
            command_sender: tokio::sync::mpsc::channel(1).0,
            capabilities,
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    token
}

async fn establish(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: pandar_core::AgentId,
    token: SessionToken,
    serial: &str,
    generation: u64,
) {
    state
        .printers()
        .establish_generation_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            serial,
            generation,
        )
        .await
        .unwrap();
}
