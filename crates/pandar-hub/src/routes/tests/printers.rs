use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use super::*;
use pandar_core::{AgentId, TenantId};
use serde_json::json;
use tokio::sync::mpsc;
use tonic::Status;
use tracing_subscriber::fmt::MakeWriter;

use crate::protocol::agent::v1::{HubCommand, hub_command};

#[tokio::test]
async fn printer_list_returns_tenant_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["printers"][0]["id"], printer_id);
    assert_eq!(body["printers"][0]["tenant_id"], tenant_id.to_string());
    assert_eq!(body["printers"][0]["agent_id"], agent_id.to_string());
    assert_eq!(body["printers"][0]["materials"], serde_json::Value::Null);
}

#[tokio::test]
async fn printer_detail_returns_tenant_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], printer_id);
    assert_eq!(body["tenant_id"], tenant_id.to_string());
    assert_eq!(body["materials"], serde_json::Value::Null);
}

#[tokio::test]
async fn tenant_admin_can_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], printer_id);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "printers": [] }));

    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.delete")
        .expect("printer delete audit event");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
}

#[tokio::test]
async fn viewer_cannot_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-delete-printer",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn printer_routes_return_material_snapshots_without_credentials() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer_id.clone(),
            serial_number: "serial".to_string(),
            printer_materials_json: json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T01:02:03Z",
                "ams_units": [{
                    "unit_id": "0",
                    "trays": [{
                        "tray_id": "0",
                        "filament_id": "GFL00",
                        "type": "PLA",
                        "color": "FF0000",
                        "access_token": "secret-token",
                        "auth": "secret-auth",
                        "passwd": "secret-passwd",
                        "access_code": "secret-access-code"
                    }]
                }],
                "external_spools": [{
                    "external_id": "254",
                    "tray_id": "0",
                    "type": "PETG"
                }],
                "active_tray": {
                    "kind": "ams",
                    "global_tray_id": 0,
                    "ams_id": "0",
                    "tray_id": "0"
                }
            })
            .to_string(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["printers"][0]["materials"]["observed_at"],
        "2026-06-23T01:02:03Z"
    );
    assert_eq!(
        body["printers"][0]["materials"]["ams_units"][0]["unit_id"],
        "0"
    );
    assert_eq!(
        body["printers"][0]["materials"]["external_spools"][0]["external_id"],
        "254"
    );
    assert_eq!(
        body["printers"][0]["materials"]["active_tray"]["kind"],
        "ams"
    );
    assert!(!body.to_string().contains("secret-token"));
    assert!(!body.to_string().contains("secret-auth"));
    assert!(!body.to_string().contains("secret-passwd"));
    assert!(!body.to_string().contains("secret-access-code"));
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains("auth"));
    assert!(!body.to_string().contains("passwd"));
    assert!(!body.to_string().contains("access_code"));

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["materials"], body["printers"][0]["materials"]);
}

#[tokio::test]
async fn printer_control_enqueues_ams_slot_operation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab X2D"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(json!({
            "action": "ams_load_filament",
            "ams_id": 0,
            "slot_id": 1,
            "global_tray_id": 1,
            "extruder_id": 0
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "printer_operation");
    let payload: serde_json::Value =
        serde_json::from_str(body["payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(payload["operation"]["type"], "ams_load_filament");
    assert_eq!(payload["operation"]["ams_id"], 0);
    assert_eq!(payload["operation"]["slot_id"], 1);
    assert_eq!(payload["operation"]["global_tray_id"], 1);
    assert_eq!(payload["operation"]["extruder_id"], 0);
}

#[tokio::test]
async fn missing_printer_detail_returns_not_found() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let printer_id = uuid::Uuid::new_v4();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "printer_not_found" }));
}

#[tokio::test]
async fn invalid_printer_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_printer_id" }));
}

#[tokio::test]
async fn refresh_printers_returns_command_record() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tenant_id"], tenant_id);
    assert_eq!(body["agent_id"], agent_id);
    assert_eq!(body["kind"], "refresh_printers");
    assert_eq!(body["status"], "queued");
    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.refresh_printers")
    );
}

#[tokio::test]
async fn refresh_printer_materials_enqueues_for_owning_agent_and_wakes_it() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let mut wake_receiver =
        register_route_test_session_with_wake(&state, tenant_id, agent_id).await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "refresh_printer_materials");
    assert_eq!(body["agent_id"], agent_id.to_string());
    assert_eq!(body["printer_id"], printer_id);
    let payload: serde_json::Value =
        serde_json::from_str(body["payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["serial_number"], format!("serial-{printer_id}"));
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");

    let audit = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|event| event.action == "printer.refresh_materials")
    );
}

#[tokio::test]
async fn refresh_printer_materials_rejects_invalid_and_missing_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_printer_id");

    let missing = uuid::Uuid::new_v4();
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{missing}/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "printer_not_found");
}

#[tokio::test]
async fn link_printer_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-link-printer-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_missing_local_session_without_command_row() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "agent_not_connected" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_missing_local_session_does_not_log_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();
    let access_code = "SECRET-LINK-CODE";

    let _ = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;
    drop(_guard);

    assert!(!logs.to_string().contains(access_code));
}

#[tokio::test]
async fn link_printer_direct_sends_secret_but_persists_only_redacted_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "link_printer");
    assert_eq!(body["status"], "sent");
    assert!(!body.to_string().contains(access_code));
    assert!(!body["payload_json"].as_str().unwrap().contains(access_code));

    let sent = command_receiver.recv().await.unwrap().unwrap();
    match sent.command.unwrap() {
        hub_command::Command::LinkPrinter(command) => {
            assert_eq!(command.printer_type, "BambuLab");
            assert_eq!(command.host, "192.0.2.10");
            assert_eq!(command.access_code, access_code);
            assert_eq!(command.name, "Office X1C");
        }
        other => panic!("expected link printer command, got {other:?}"),
    }

    let payload: serde_json::Value =
        serde_json::from_str(body["payload_json"].as_str().unwrap()).unwrap();
    assert_eq!(payload["printer_type"], "BambuLab");
    assert_eq!(payload["host"], "192.0.2.10");
    assert_eq!(payload["access_code"], "[redacted]");
    assert_eq!(payload["name"], "Office X1C");
    assert!(payload.get("serial_number").is_none());
    assert!(payload.get("model").is_none());
}

#[tokio::test]
async fn link_printer_maps_absent_or_blank_optional_name_to_empty_proto_string() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;

    for body in [
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "192.0.2.11", "access_code": "SECRET-LINK-CODE", "name": "   " }),
    ] {
        let (status, response) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let sent = command_receiver.recv().await.unwrap().unwrap();
        match sent.command.unwrap() {
            hub_command::Command::LinkPrinter(command) => {
                assert_eq!(command.name, "");
            }
            other => panic!("expected link printer command, got {other:?}"),
        }
        assert_eq!(response["status"], "sent");
    }
}

#[tokio::test]
async fn link_printer_marks_command_failed_when_live_channel_closed_after_row_creation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let (command_sender, command_receiver) = tokio::sync::mpsc::channel(1);
    drop(command_receiver);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "link_printer");
    assert_eq!(body["status"], "failed");
    assert_eq!(
        body["error"],
        "agent command channel unavailable before printer link completed"
    );
    assert!(!body.to_string().contains(access_code));
    assert_eq!(state.commands().count().await.unwrap(), 1);
    let command_id = pandar_core::CommandId::parse(body["id"].as_str().unwrap()).unwrap();
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, pandar_core::CommandStatus::Failed);
    assert_eq!(
        stored.error.as_deref(),
        Some("agent command channel unavailable before printer link completed")
    );
    assert!(
        !state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn link_printer_rejects_blank_required_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    for body in [
        json!({ "type": "BambuLab", "host": "", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "" }),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "bad_request" }));
    }

    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_invalid_type_host_and_legacy_metadata_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    for request in [
        json!({ "type": "", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "Other", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "printer.local", "access_code": "SECRET-LINK-CODE" }),
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE", "serial_number": "SERIAL123" }),
        json!({ "type": "BambuLab", "host": "192.0.2.10", "access_code": "SECRET-LINK-CODE", "model": "X1 Carbon" }),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(request),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "bad_request");
    }
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_unknown_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(json!({
            "type": "BambuLab",
            "host": "192.0.2.10",
            "access_code": "SECRET-LINK-CODE",
            "unexpected": true
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

fn link_printer_body(access_code: &str) -> serde_json::Value {
    json!({
        "type": "BambuLab",
        "host": "192.0.2.10",
        "access_code": access_code,
        "name": "Office X1C"
    })
}

async fn register_route_test_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) {
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;
}

async fn register_route_test_session_with_wake(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> mpsc::Receiver<()> {
    let (wake_sender, wake_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender: mpsc::channel(1).0,
            command_sender: mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;
    wake_receiver
}

#[derive(Clone)]
struct CapturedLogs {
    output: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn writer(&self) -> TestLogWriter {
        TestLogWriter {
            output: self.output.clone(),
        }
    }
}

impl std::fmt::Display for CapturedLogs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.output.lock().unwrap().clone();
        formatter.write_str(&String::from_utf8_lossy(&output))
    }
}

#[derive(Clone)]
struct TestLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> MakeWriter<'writer> for TestLogWriter {
    type Writer = TestLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        TestLogBuffer {
            output: self.output.clone(),
        }
    }
}

struct TestLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
