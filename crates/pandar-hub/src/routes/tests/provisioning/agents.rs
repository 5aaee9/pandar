use super::*;

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

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, "agent.pairing_bundle");
    assert_eq!(
        events[0].metadata_json,
        json!({ "agent_name": "workshop-agent" }).to_string()
    );
}
