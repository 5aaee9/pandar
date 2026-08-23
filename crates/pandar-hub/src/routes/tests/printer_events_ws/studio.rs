use std::time::Duration;

use super::*;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StudioFrame {
    SnapshotBegin {
        version: u32,
    },
    PrinterUpsert {
        printer: Value,
    },
    SnapshotEnd,
    PrinterRemoved {
        dev_id: String,
        pandar_printer_id: String,
    },
}

struct StudioFixture {
    state: AppState,
    tenant: pandar_core::Tenant,
    agent_id: AgentId,
    printer_id: String,
    serial_number: String,
    token: String,
}

async fn studio_fixture(slug: &str) -> StudioFixture {
    let state = state().await;
    let tenant = state.tenants().create(slug, slug).await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "studio-ws-token").await;
    StudioFixture {
        state,
        tenant,
        agent_id: agent.id,
        serial_number: format!("serial-{printer_id}"),
        printer_id,
        token,
    }
}

async fn connect_studio(
    http_addr: std::net::SocketAddr,
    tenant_id: TenantId,
    query: &str,
    token: Option<&str>,
) -> impl tokio_stream::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin
{
    let mut request = format!("ws://{http_addr}/api/v1/tenants/{tenant_id}/printer-events{query}")
        .into_client_request()
        .unwrap();
    if let Some(token) = token {
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    }
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn next_frame<S>(socket: &mut S, context: &str) -> StudioFrame
where
    S: tokio_stream::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for studio frame: {context}"))
            .expect("studio websocket closed")
            .expect("studio websocket error");
        match message {
            Message::Text(_) => return decode_ws_message::<StudioFrame>(message),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected studio websocket message: {other:?}"),
        }
    }
}

async fn assert_studio_quiet<S>(socket: &mut S, context: &str)
where
    S: tokio_stream::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    while let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(remaining, socket.next()).await {
            Err(_) => return,
            Ok(None) => return,
            Ok(Some(Err(err))) => panic!("studio websocket error: {err}"),
            Ok(Some(Ok(Message::Text(text)))) => {
                panic!("unexpected studio frame: {text} ({context})");
            }
            Ok(Some(Ok(_))) => {}
        }
    }
}

mod changes;
mod lifecycle;
mod replication;
mod snapshot;
