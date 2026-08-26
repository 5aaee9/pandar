use std::{sync::Arc, time::Duration};

use anyhow::{Context, bail};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::transport::Server;

use crate::{
    AppState,
    grpc::AgentControlService,
    grpc_connection_limit,
    routes::{self, ApiError},
    runtime,
};
use pandar_protocol::agent::v1::agent_control_server::AgentControlServer;

pub(crate) fn authorize_bootstrap(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if state.no_auth_enabled() {
        return Ok(());
    }

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
    let bind_addr =
        std::env::var("PANDAR_HUB_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let grpc_bind_addr =
        std::env::var("PANDAR_HUB_GRPC_BIND").unwrap_or_else(|_| "127.0.0.1:50051".to_owned());
    let observability_bind_addr = std::env::var("PANDAR_HUB_OBSERVABILITY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:9090".to_owned());
    let max_unauthenticated_grpc_connections_per_peer = std::env::var(
        "PANDAR_HUB_GRPC_MAX_UNAUTHENTICATED_CONNECTIONS_PER_PEER",
    )
    .unwrap_or_else(|_| "64".to_owned())
    .parse::<usize>()
    .ok()
    .filter(|value| *value > 0)
    .context(
        "PANDAR_HUB_GRPC_MAX_UNAUTHENTICATED_CONNECTIONS_PER_PEER must be a positive integer",
    )?;
    let parsed_grpc_bind = grpc_bind_addr
        .parse::<std::net::SocketAddr>()
        .with_context(|| format!("invalid pandar-hub gRPC bind address {grpc_bind_addr}"))?;
    let grpc_tls_config = grpc_tls_config().await?;
    if !parsed_grpc_bind.ip().is_loopback() && grpc_tls_config.is_none() {
        bail!(
            "PANDAR_HUB_GRPC_TLS_CERT and PANDAR_HUB_GRPC_TLS_KEY are required for a non-loopback gRPC bind"
        );
    }
    let database_url =
        std::env::var("PANDAR_DATABASE_URL").unwrap_or_else(|_| "sqlite://pandar.db".to_owned());
    let state = AppState::connect(database_url)
        .await
        .context("failed to initialize pandar-hub application state")?;
    if state.no_auth_enabled() {
        tracing::warn!(
            "PANDAR_HUB_NO_AUTH=true; pandar-hub HTTP authentication and authorization are disabled"
        );
    }
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub to {bind_addr}"))?;
    let grpc_listener = TcpListener::bind(&grpc_bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub gRPC to {grpc_bind_addr}"))?;
    let observability_listener = TcpListener::bind(&observability_bind_addr)
        .await
        .with_context(|| {
            format!("failed to bind pandar-hub observability to {observability_bind_addr}")
        })?;

    tracing::info!(%bind_addr, "pandar-hub listening");
    tracing::info!(%grpc_bind_addr, "pandar-hub gRPC listening");
    tracing::info!(%observability_bind_addr, "pandar-hub observability listening");
    let _session_expiry = runtime::spawn_session_expiry(state.clone());
    let _control_plane = start_control_plane(state.clone()).await?;
    let http = axum::serve(listener, routes::router(state.clone()));
    let observability = axum::serve(
        observability_listener,
        routes::observability_router(state.clone()),
    );
    let grpc = Server::builder()
        .concurrency_limit_per_connection(16)
        .load_shed(true)
        .max_concurrent_streams(16_u32)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        .add_service(
            AgentControlServer::new(AgentControlService::new(state))
                .max_decoding_message_size(1024 * 1024),
        )
        .serve_with_incoming(grpc_connection_limit::incoming(
            grpc_listener,
            1024,
            max_unauthenticated_grpc_connections_per_peer,
            grpc_tls_config,
        ));

    tokio::try_join!(
        async { http.await.context("pandar-hub HTTP server exited") },
        async {
            observability
                .await
                .context("pandar-hub observability server exited")
        },
        async { grpc.await.context("pandar-hub gRPC server exited") },
    )?;

    Ok(())
}

async fn grpc_tls_config() -> anyhow::Result<Option<Arc<ServerConfig>>> {
    let (cert_path, key_path) = match (
        std::env::var("PANDAR_HUB_GRPC_TLS_CERT").ok(),
        std::env::var("PANDAR_HUB_GRPC_TLS_KEY").ok(),
    ) {
        (None, None) => return Ok(None),
        (Some(cert_path), Some(key_path)) => (cert_path, key_path),
        _ => {
            bail!(
                "PANDAR_HUB_GRPC_TLS_CERT and PANDAR_HUB_GRPC_TLS_KEY must be configured together"
            )
        }
    };
    let cert = tokio::fs::read(&cert_path)
        .await
        .with_context(|| format!("failed to read gRPC TLS certificate {cert_path}"))?;
    let key = tokio::fs::read(&key_path)
        .await
        .with_context(|| format!("failed to read gRPC TLS private key {key_path}"))?;
    let certificates = CertificateDer::pem_slice_iter(&cert)
        .collect::<Result<Vec<_>, _>>()
        .context("failed to parse gRPC TLS certificate chain")?;
    let private_key =
        PrivateKeyDer::from_pem_slice(&key).context("failed to parse gRPC TLS private key")?;
    let mut config =
        ServerConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .context("failed to configure gRPC TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)
            .context("failed to configure gRPC TLS certificate and private key")?;
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(Some(Arc::new(config)))
}

async fn start_control_plane(state: AppState) -> anyhow::Result<JoinHandle<()>> {
    let (control_plane, control_plane_ready) = runtime::spawn_control_plane_ready(state);
    control_plane_ready
        .await
        .context("control plane subscriber stopped before reporting readiness")?
        .context("failed to start control plane subscriber")?;
    Ok(control_plane)
}

#[cfg(test)]
mod tests;
