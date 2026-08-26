use std::time::Duration;

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;

use crate::AgentConfig;
use pandar_protocol::agent::v1::agent_control_client::AgentControlClient;

use super::{camera_hello_event, send_camera_closed};

pub(super) fn spawn_reverse_camera_closed(
    config: AgentConfig,
    stream_id: String,
    success: bool,
    error: String,
) -> JoinHandle<()> {
    tokio::spawn(report_reverse_camera_closed(
        config, stream_id, success, error,
    ))
}

pub(super) async fn report_reverse_camera_closed(
    config: AgentConfig,
    stream_id: String,
    success: bool,
    error: String,
) {
    match tokio::time::timeout(
        Duration::from_secs(10),
        reverse_camera_closed(config, stream_id, success, error),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!(error = %format!("{err:#}"), "on-demand camera close event failed");
        }
        Err(err) => {
            tracing::warn!(error = %err, "on-demand camera close event timed out");
        }
    }
}

async fn reverse_camera_closed(
    config: AgentConfig,
    stream_id: String,
    success: bool,
    error: String,
) -> anyhow::Result<()> {
    let mut client = AgentControlClient::connect(config.hub_grpc_url.clone())
        .await
        .with_context(|| {
            format!(
                "connect on-demand camera close event to hub gRPC at {}",
                config.hub_grpc_url
            )
        })?;
    let (sender, receiver) = mpsc::channel(2);
    sender
        .send(camera_hello_event(&config))
        .await
        .context("queue agent camera hello event")?;
    send_camera_closed(&config, &sender, &stream_id, success, error).await;
    drop(sender);
    client
        .reverse_camera(Request::new(ReceiverStream::new(receiver)))
        .await
        .context("open reverse agent camera stream for close event")?;
    Ok(())
}
