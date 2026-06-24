use std::net::SocketAddr;

use anyhow::{Context, ensure};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use pandar_agent::{
    AgentConfig,
    commands::{ArtifactReader, HubArtifactReader},
};
use pandar_hub::grpc::commands::next_hub_command_for_agent;
use pandar_hub::{AppState, router};
use serde_json::Value;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use tower::ServiceExt;

use crate::fixture::{ARTIFACT_BYTES, SmokeFixture, SmokeWorld};

pub async fn create_print_through_multipart_route(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<StatusCode> {
    let (status, body) = create_print_raw(state, fixture).await?;
    ensure_created(status, &body)?;
    Ok(status)
}

async fn create_print_raw(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<(StatusCode, Vec<u8>)> {
    let body = multipart_print_body(&fixture.printer_id, "scaled-smoke.3mf");
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/plugin/prints")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", fixture.plugin_token),
        )
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", body.boundary),
        )
        .body(Body::from(body.body))
        .context("build multipart print request")?;
    let response = router(state.clone())
        .oneshot(request)
        .await
        .context("send multipart print request")?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    Ok((status, body.to_vec()))
}

fn ensure_created(status: StatusCode, body: &[u8]) -> anyhow::Result<()> {
    ensure!(
        status == StatusCode::CREATED,
        "multipart print route returned {status}: {}",
        String::from_utf8_lossy(body)
    );
    Ok(())
}

pub async fn create_print_expect_status(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<StatusCode> {
    let body = multipart_print_body(&fixture.printer_id, "scaled-smoke.3mf");
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/plugin/prints")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", fixture.plugin_token),
        )
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", body.boundary),
        )
        .body(Body::from(body.body))
        .context("build multipart print request")?;
    let response = router(state.clone())
        .oneshot(request)
        .await
        .context("send multipart print request")?;
    Ok(response.status())
}

pub async fn create_ws_ticket(
    state: &AppState,
    tenant_id: pandar_core::TenantId,
    bearer: &str,
) -> anyhow::Result<String> {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/tenants/{tenant_id}/printer-events/tickets"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::empty())
        .context("build printer event ticket request")?;
    let response = router(state.clone())
        .oneshot(request)
        .await
        .context("send printer event ticket request")?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes();
    ensure!(
        status == StatusCode::OK,
        "printer event ticket route returned {status}: {}",
        String::from_utf8_lossy(&body)
    );
    let json: Value = serde_json::from_slice(&body).context("decode printer event ticket body")?;
    json["ticket"]
        .as_str()
        .map(str::to_owned)
        .context("printer event ticket response missing ticket")
}

pub async fn download_artifact_route(
    state: &AppState,
    fixture: &SmokeFixture,
    path: &str,
) -> anyhow::Result<(StatusCode, Vec<u8>)> {
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", fixture.agent_credential),
        )
        .body(Body::empty())
        .context("build artifact download request")?;
    let response = router(state.clone())
        .oneshot(request)
        .await
        .context("send artifact download request")?;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes().to_vec();
    Ok((status, body))
}

pub async fn download_artifact(
    state: &AppState,
    world: &SmokeWorld,
    fixture: &SmokeFixture,
    path: &str,
) -> anyhow::Result<()> {
    let base_url = serve_hub(state.clone()).await?;
    let agent_config = AgentConfig {
        hub_grpc_url: "grpc://unused-in-smoke".to_owned(),
        hub_api_url: Some(base_url),
        agent_name: "scaled-smoke-agent".to_owned(),
        agent_id: fixture.agent_id.to_string(),
        tenant_id: fixture.tenant_id.to_string(),
        agent_credential: fixture.agent_credential.clone(),
        agent_version: "scaled-smoke".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: world.temp.path().join("agent-artifacts"),
    };
    let downloaded = HubArtifactReader::new(&agent_config)
        .read_artifact(path)
        .await?;
    ensure!(
        downloaded == ARTIFACT_BYTES,
        "downloaded artifact bytes differ"
    );
    Ok(())
}

pub async fn dequeue_print_command(
    state: &AppState,
    fixture: &SmokeFixture,
) -> anyhow::Result<(
    pandar_core::CommandId,
    pandar_hub::protocol::agent::v1::PrintProjectFile,
)> {
    ensure!(
        state.commands().count().await? >= 1,
        "Hub B did not see the queued command in the shared database"
    );
    let command = next_hub_command_for_agent(state, fixture.tenant_id, fixture.agent_id)
        .await
        .map_err(|status| anyhow::anyhow!("command conversion failed: {status}"))?
        .context("expected queued command")?;
    let command_id = pandar_core::CommandId::parse(&command.command_id)?;
    match command.command.context("expected hub command payload")? {
        pandar_hub::protocol::agent::v1::hub_command::Command::PrintProjectFile(print) => {
            Ok((command_id, print))
        }
        _ => anyhow::bail!("expected PrintProjectFile command"),
    }
}

pub async fn connect_ws_with_ticket(
    base_url: &str,
    tenant_id: pandar_core::TenantId,
    ticket: &str,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let ws_url = format!(
        "{}/api/v1/tenants/{tenant_id}/printer-events?ticket={ticket}",
        base_url.replacen("http://", "ws://", 1)
    );
    let request = ws_url.into_client_request()?;
    let (ws, _) = connect_async(request).await.context("connect websocket")?;
    Ok(ws)
}

pub async fn next_ws_event_type(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> anyhow::Result<String> {
    let message = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .context("websocket event timed out")?
        .context("websocket closed before event")?
        .context("websocket read failed")?;
    let text = message.into_text().context("expected websocket text")?;
    let json: Value = serde_json::from_str(&text).context("decode websocket event")?;
    json["type"]
        .as_str()
        .map(str::to_owned)
        .context("websocket event missing type")
}

pub async fn serve_hub(state: AppState) -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router(state)).await {
            eprintln!("scaled smoke hub server failed: {err:#}");
        }
    });
    Ok(format!("http://{addr}"))
}

struct MultipartBody {
    boundary: String,
    body: Vec<u8>,
}

fn multipart_print_body(printer_id: &str, filename: &str) -> MultipartBody {
    let boundary = "pandar-scaled-smoke-boundary";
    let mut body = Vec::new();
    for (name, value) in [
        ("printer_id", printer_id),
        ("filename", filename),
        ("content_type", "model/3mf"),
        ("plate_id", "1"),
        ("use_ams", "true"),
        ("flow_cali", "false"),
        ("timelapse", "true"),
        ("ams_mapping", "[]"),
    ] {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: model/3mf\r\n\r\n");
    body.extend_from_slice(ARTIFACT_BYTES);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    MultipartBody {
        boundary: boundary.to_owned(),
        body,
    }
}
