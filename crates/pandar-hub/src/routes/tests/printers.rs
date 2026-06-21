use super::*;
use crate::{
    grpc::AgentControlService,
    protocol::agent::v1::{
        AgentEvent, AgentHello, PrinterSnapshot, agent_control_client::AgentControlClient,
        agent_control_server::AgentControlServer, agent_event,
    },
};
use pandar_core::AgentId;
use tokio::net::TcpListener;
use tokio_stream::{
    StreamExt,
    wrappers::{ReceiverStream, TcpListenerStream},
};
use tokio_tungstenite::tungstenite::Message;
use tonic::transport::Server;

#[tokio::test]
async fn printer_list_returns_tenant_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent) = tenant_and_agent(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["printers"][0]["id"], printer_id);
    assert_eq!(body["printers"][0]["tenant_id"], tenant_id.to_string());
    assert_eq!(body["printers"][0]["agent_id"], agent_id.to_string());
}

#[tokio::test]
async fn printer_detail_returns_tenant_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent) = tenant_and_agent(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], printer_id);
    assert_eq!(body["tenant_id"], tenant_id.to_string());
}

#[tokio::test]
async fn missing_printer_detail_returns_not_found() {
    let app = app().await;
    let (tenant, _) = tenant_and_agent(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let printer_id = uuid::Uuid::new_v4();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "printer_not_found" }));
}

#[tokio::test]
async fn invalid_printer_id_returns_bad_request() {
    let app = app().await;
    let (tenant, _) = tenant_and_agent(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_printer_id" }));
}

#[tokio::test]
async fn refresh_printers_returns_command_record() {
    let app = app().await;
    let (tenant, agent) = tenant_and_agent(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tenant_id"], tenant_id);
    assert_eq!(body["agent_id"], agent_id);
    assert_eq!(body["kind"], "refresh_printers");
    assert_eq!(body["status"], "queued");
}

#[tokio::test]
async fn invalid_agent_id_on_refresh_returns_bad_request() {
    let app = app().await;
    let (tenant, _) = tenant_and_agent(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid/refresh-printers"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_agent_id" }));
}

#[tokio::test]
async fn printer_events_invalid_tenant_returns_bad_request_before_upgrade() {
    let (status, body) = request(
        app().await,
        Method::GET,
        "/api/v1/tenants/not-a-uuid/printer-events",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));
}

#[tokio::test]
async fn printer_events_missing_tenant_returns_not_found_before_upgrade() {
    let tenant_id = TenantId::new();

    let (status, body) = request(
        app().await,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printer-events"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "tenant_not_found" }));
}

#[tokio::test]
async fn printer_events_websocket_receives_snapshot_from_grpc_stream() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let http_addr = serve_http(app).await;
    let grpc_addr = serve_grpc(state).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    ))
    .await
    .unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant.id, agent.id)).await.unwrap();
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    sender
        .send(snapshot_event(tenant.id, agent.id))
        .await
        .unwrap();

    let message = ws.next().await.unwrap().unwrap();
    let body: Value = match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        other => panic!("expected text websocket message, got {other:?}"),
    };

    assert_eq!(body["type"], "printer_snapshot");
    assert_eq!(body["printer"]["tenant_id"], tenant.id.to_string());
    assert_eq!(body["printer"]["agent_id"], agent.id.to_string());
    assert_eq!(body["printer"]["serial_number"], "SN-001");
    drop(stream);
}

async fn serve_http(app: Router) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn serve_grpc(state: AppState) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(AgentControlServer::new(AgentControlService::new(state)))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    addr
}

fn hello_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::Hello(AgentHello {
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
        })),
    }
}

fn snapshot_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: "SN-001".to_string(),
            name: "X1 Carbon".to_string(),
            model: "X1C".to_string(),
            state: "idle".to_string(),
        })),
    }
}
