use super::*;
use crate::entities::agents;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tokio::sync::mpsc;

#[tokio::test]
async fn tenant_admin_can_create_agent_pairing_bundle() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
        Some(json!({ "name": "workshop-agent" })),
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["agent"]["tenant_id"], tenant_id);
    assert_eq!(body["agent"]["name"], "workshop-agent");
    assert_eq!(body["agent"]["status"], "offline");
    let agent_id = body["agent"]["id"].as_str().unwrap();
    let agent_env = body["agent_env"].as_str().unwrap();
    assert!(agent_env.contains(&format!("PANDAR_TENANT_ID={tenant_id}\n")));
    assert!(agent_env.contains(&format!("PANDAR_AGENT_ID={agent_id}\n")));
    assert!(agent_env.contains("PANDAR_AGENT_NAME=workshop-agent\n"));
    let credential_line = agent_env
        .lines()
        .find(|line| line.starts_with("PANDAR_AGENT_CREDENTIAL="))
        .unwrap();
    let credential = credential_line
        .strip_prefix("PANDAR_AGENT_CREDENTIAL=")
        .unwrap();
    assert!(credential.starts_with("pandar_ac_"));

    let stored_agent = agents::Entity::find()
        .filter(agents::Column::Id.eq(agent_id))
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_agent.credential_hash,
        Some(crate::repositories::hash_token_for_test(credential))
    );
    assert!(stored_agent.credential_rotated_at.is_some());

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, "agent.pairing_bundle");
    assert_eq!(events[0].actor_type, "tenant_token");
    let metadata = serde_json::from_str::<serde_json::Value>(&events[0].metadata_json).unwrap();
    assert_eq!(metadata["agent_name"], "workshop-agent");
    assert!(metadata["tenant_token_id"].as_str().is_some());
    assert_eq!(metadata["tenant_token_scopes"], json!(["*"]));
}

#[tokio::test]
async fn agent_pairing_rejects_env_line_breaks_in_name() {
    let (_state, app, tenant_id, admin_token) = admin_tenant().await;

    for name in [
        "bad\nPANDAR_AGENT_CREDENTIAL=pandar_ac_injected",
        "bad\rname",
        "bad\0name",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
            Some(json!({ "name": name })),
            &admin_token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "bad_request" }));
    }
}

#[tokio::test]
async fn tenant_admin_can_rotate_agent_credential_once() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);
    let (status, paired) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
        Some(json!({ "name": "rotating-agent" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let agent_id = paired["agent"]["id"].as_str().unwrap();
    let old_credential = paired["agent_env"]
        .as_str()
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("PANDAR_AGENT_CREDENTIAL="))
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate"),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["agent"]["id"], agent_id);
    let credential = body["credential"].as_str().unwrap();
    assert!(credential.starts_with("pandar_ac_"));
    assert_ne!(credential, old_credential);
    assert!(body["agent"].get("credential_hash").is_none());
    let stored_agent = agents::Entity::find()
        .filter(agents::Column::Id.eq(agent_id))
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_agent.credential_hash,
        Some(crate::repositories::hash_token_for_test(credential))
    );
    assert_eq!(stored_agent.credential_revoked_at, None);

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.credential_rotate")
        .unwrap();
    assert_eq!(event.actor_type, "tenant_token");
    assert_eq!(event.target_type, "agent");
    assert_eq!(event.target_id.as_deref(), Some(agent_id));
    let metadata = serde_json::from_str::<serde_json::Value>(&event.metadata_json).unwrap();
    assert!(metadata.get("tenant_token_id").is_some());
    assert_eq!(metadata["tenant_token_scopes"], json!(["*"]));
    assert!(metadata.get("credential").is_none());
    assert!(metadata.get("credential_hash").is_none());
}

#[tokio::test]
async fn all_scope_tenant_token_can_rotate_and_revoke_agent_credential() {
    let (state, app, tenant_id, _admin_token) = admin_tenant().await;
    let all_token = all_scope_tenant_token(&state, &tenant_id, "agent-credential-all").await;
    let (status, paired) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
        Some(json!({ "name": "all-scope-agent" })),
        &all_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let agent_id = paired["agent"]["id"].as_str().unwrap();

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate"),
        None,
        &all_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revoked) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:revoke"),
        None,
        &all_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["id"], agent_id);
    let stored_agent = agents::Entity::find()
        .filter(agents::Column::Id.eq(agent_id))
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert!(stored_agent.credential_revoked_at.is_some());

    let tenant = pandar_core::TenantId::parse(&tenant_id).unwrap();
    let events = state.audit_events().list_for_tenant(tenant).await.unwrap();
    assert!(events.iter().any(|event| {
        event.action == "agent.credential_rotate"
            && event.actor_type == "tenant_token"
            && event.target_id.as_deref() == Some(agent_id)
    }));
    let revoke = events
        .iter()
        .find(|event| event.action == "agent.credential_revoke")
        .unwrap();
    assert_eq!(revoke.actor_type, "tenant_token");
    assert_eq!(revoke.target_id.as_deref(), Some(agent_id));
    let metadata = serde_json::from_str::<serde_json::Value>(&revoke.metadata_json).unwrap();
    assert!(metadata.get("tenant_token_id").is_some());
    assert_eq!(metadata["tenant_token_scopes"], json!(["*"]));
    assert!(metadata.get("credential").is_none());
    assert!(metadata.get("credential_hash").is_none());
}

#[tokio::test]
async fn external_tenant_admin_rotate_and_revoke_audit_user_actor() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let admin_token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "agent-credential-user",
    )
    .await;
    let (status, paired) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/agent-pairings", tenant.id),
        Some(json!({ "name": "user-audited-agent" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let agent_id = paired["agent"]["id"].as_str().unwrap();

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/agents/{agent_id}/credential:rotate",
            tenant.id
        ),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/agents/{agent_id}/credential:revoke",
            tenant.id
        ),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    for action in ["agent.credential_rotate", "agent.credential_revoke"] {
        let event = events.iter().find(|event| event.action == action).unwrap();
        assert_eq!(event.actor_type, "user");
        assert!(event.user_id.is_some());
        assert_eq!(event.target_id.as_deref(), Some(agent_id));
        let metadata = serde_json::from_str::<serde_json::Value>(&event.metadata_json).unwrap();
        assert!(metadata.get("tenant_token_id").is_none());
        assert!(metadata.get("credential").is_none());
        assert!(metadata.get("credential_hash").is_none());
    }
}

#[tokio::test]
async fn agent_register_tenant_token_can_rotate_and_revoke_agent_credential() {
    let (state, app, tenant_id, _admin_token) = admin_tenant().await;
    let register_token =
        agent_register_tenant_token(&state, &tenant_id, "agent-credential-register").await;
    let all_token = all_scope_tenant_token(&state, &tenant_id, "agent-credential-bootstrap").await;
    let (status, paired) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
        Some(json!({ "name": "register-scope-agent" })),
        &all_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let agent_id = paired["agent"]["id"].as_str().unwrap();

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate"),
        None,
        &register_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revoked) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:revoke"),
        None,
        &register_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["id"], agent_id);
}

#[tokio::test]
async fn agent_credential_revoke_closes_current_session() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let agent = state
        .agents()
        .create(
            pandar_core::TenantId::parse(&tenant_id).unwrap(),
            "live-agent",
        )
        .await
        .unwrap();
    let credential = "pandar_ac_live";
    state
        .agents()
        .rotate_credential(agent.tenant_id, agent.id, credential, test_audit_actor())
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: agent.tenant_id,
            agent_id: agent.id,
            name: agent.name.clone(),
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;

    let (status, _) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/agents/{}/credential:revoke",
            agent.id
        ),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    tokio::time::timeout(std::time::Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("agent session should be closed")
        .expect("close channel should stay open");
    assert!(state.sessions().get(agent.id).await.is_none());
}

#[tokio::test]
async fn agent_credential_rotate_returns_secret_when_close_publish_fails() {
    let (state, _app, tenant_id, admin_token) = admin_tenant().await;
    let state =
        state.with_control_plane_for_tests(crate::cluster::ControlPlane::failing_for_tests());
    let app = router(state.clone());
    let agent = state
        .agents()
        .create(
            pandar_core::TenantId::parse(&tenant_id).unwrap(),
            "close-publish-failure-agent",
        )
        .await
        .unwrap();

    let (status, rotated) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/agents/{}/credential:rotate",
            agent.id
        ),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(rotated["agent"]["id"], agent.id.to_string());
    assert!(
        rotated["credential"]
            .as_str()
            .unwrap()
            .starts_with(crate::repositories::AGENT_CREDENTIAL_PREFIX)
    );
}

#[tokio::test]
async fn agent_credential_rotate_closes_current_session() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let agent = state
        .agents()
        .create(
            pandar_core::TenantId::parse(&tenant_id).unwrap(),
            "rotating-live-agent",
        )
        .await
        .unwrap();
    let credential = "pandar_ac_rotate_live";
    state
        .agents()
        .rotate_credential(agent.tenant_id, agent.id, credential, test_audit_actor())
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: agent.tenant_id,
            agent_id: agent.id,
            name: agent.name.clone(),
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;

    let (status, _) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/agents/{}/credential:rotate",
            agent.id
        ),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    tokio::time::timeout(std::time::Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("agent session should be closed")
        .expect("close channel should stay open");
    assert!(state.sessions().get(agent.id).await.is_none());
}

#[tokio::test]
async fn agent_credential_revoke_closes_sibling_session() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let agent = state
        .agents()
        .create(
            pandar_core::TenantId::parse(&tenant_id).unwrap(),
            "sibling-live-agent",
        )
        .await
        .unwrap();
    let credential = "pandar_ac_live";
    state
        .agents()
        .rotate_credential(agent.tenant_id, agent.id, credential, test_audit_actor())
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: agent.tenant_id,
            agent_id: agent.id,
            name: agent.name.clone(),
            version: "0.1.0".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;

    let (status, revoked) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/agents/{}/credential:revoke",
            agent.id
        ),
        None,
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["id"], agent.id.to_string());
    tokio::time::timeout(std::time::Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("sibling agent session should be closed")
        .expect("close channel should stay open");
    assert!(sibling.sessions().get(agent.id).await.is_none());
}

fn test_audit_actor() -> crate::repositories::AuditActor {
    crate::repositories::AuditActor::tenant_token(None, "test-setup-token", vec!["*"])
}
