use anyhow::Context;
use pandar_hub::{
    AppState, grpc::AgentControlService,
    protocol::agent::v1::agent_control_server::AgentControlServer, router,
    runtime::spawn_session_expiry,
};
use tokio::net::TcpListener;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

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
    let _session_expiry = spawn_session_expiry(state.clone());
    let http = axum::serve(listener, router(state.clone()));
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
