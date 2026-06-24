use anyhow::Context;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use tokio::net::TcpListener;
use tonic::transport::Server;

use crate::{
    AppState,
    grpc::AgentControlService,
    protocol::agent::v1::agent_control_server::AgentControlServer,
    routes::{self, ApiError},
    runtime,
};

pub(crate) fn authorize_bootstrap(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    let Some(configured_token) = state.bootstrap_token() else {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "bootstrap_disabled"));
    };
    if token != configured_token {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    }

    Ok(())
}

pub async fn run_from_env() -> anyhow::Result<()> {
    let bind_addr = std::env::var("PANDAR_HUB_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let grpc_bind_addr =
        std::env::var("PANDAR_HUB_GRPC_BIND").unwrap_or_else(|_| "0.0.0.0:50051".to_owned());
    let database_url =
        std::env::var("PANDAR_DATABASE_URL").unwrap_or_else(|_| "sqlite://pandar.db".to_owned());
    let state = AppState::connect(database_url)
        .await
        .context("failed to initialize pandar-hub application state")?;
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub to {bind_addr}"))?;
    let grpc_listener = TcpListener::bind(&grpc_bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub gRPC to {grpc_bind_addr}"))?;

    tracing::info!(%bind_addr, "pandar-hub listening");
    tracing::info!(%grpc_bind_addr, "pandar-hub gRPC listening");
    let _session_expiry = runtime::spawn_session_expiry(state.clone());
    let (_control_plane, control_plane_ready) = runtime::spawn_control_plane_ready(state.clone());
    control_plane_ready
        .await
        .context("control plane subscriber stopped before reporting readiness")?
        .context("failed to start control plane subscriber")?;
    let http = axum::serve(listener, routes::router(state.clone()));
    let grpc = Server::builder()
        .add_service(AgentControlServer::new(AgentControlService::new(state)))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(
            grpc_listener,
        ));

    tokio::try_join!(
        async { http.await.context("pandar-hub HTTP server exited") },
        async { grpc.await.context("pandar-hub gRPC server exited") },
    )?;

    Ok(())
}
