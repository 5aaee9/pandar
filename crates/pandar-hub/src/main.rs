use anyhow::Context;
use pandar_hub::{AppState, router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = std::env::var("PANDAR_HUB_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let database_url =
        std::env::var("PANDAR_DATABASE_URL").unwrap_or_else(|_| "sqlite://pandar.db".to_owned());
    let state = AppState::connect(database_url)
        .await
        .context("failed to initialize pandar-hub application state")?;
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub to {bind_addr}"))?;

    tracing::info!(%bind_addr, "pandar-hub listening");
    axum::serve(listener, router(state))
        .await
        .context("pandar-hub server exited")?;

    Ok(())
}
