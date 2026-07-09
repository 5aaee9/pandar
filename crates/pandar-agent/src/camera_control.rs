use std::collections::HashMap;

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::Request;

use crate::{
    AgentConfig,
    machine::{
        BambuMachineGateway,
        camera::{stream_camera_fragmented_mp4, stream_camera_mjpeg},
    },
    protocol::agent::v1::{
        AgentCameraChunk, AgentCameraClosed, AgentCameraEvent, AgentCameraHello, CameraStreamMode,
        HubCameraCommand, agent_camera_event, agent_control_client::AgentControlClient,
        hub_camera_command,
    },
};

pub fn camera_hello_event(config: &AgentConfig) -> AgentCameraEvent {
    camera_event(
        config,
        "camera-hello",
        agent_camera_event::Event::Hello(AgentCameraHello {
            credential: config.agent_credential.clone(),
        }),
    )
}

pub async fn handle_control_camera_command<G>(
    config: &AgentConfig,
    gateway: &G,
    streams: &mut HashMap<String, JoinHandle<()>>,
    command: HubCameraCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    match command.command {
        Some(hub_camera_command::Command::Open(open)) => {
            if let Some(task) = streams.remove(&command.stream_id) {
                task.abort();
            }
            let stream_id = command.stream_id.clone();
            let mode = CameraStreamMode::try_from(open.mode).unwrap_or(CameraStreamMode::Mjpeg);
            let task = match gateway.camera_endpoint(&open.serial_number).await {
                Ok(endpoint) => {
                    spawn_reverse_camera_stream(config.clone(), stream_id.clone(), endpoint, mode)
                }
                Err(err) => {
                    let error = gateway.redact_error(&format!("{err:#}"));
                    spawn_reverse_camera_closed(config.clone(), stream_id.clone(), false, error)
                }
            };
            streams.insert(stream_id, task);
        }
        Some(hub_camera_command::Command::Close(_)) => {
            if let Some(task) = streams.remove(&command.stream_id) {
                task.abort();
            }
        }
        None => {}
    }

    Ok(())
}

fn spawn_reverse_camera_stream(
    config: AgentConfig,
    stream_id: String,
    endpoint: crate::machine::BambuPrinterEndpoint,
    mode: CameraStreamMode,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(err) = reverse_camera_stream(config, stream_id, endpoint, mode).await {
            tracing::warn!(error = %format!("{err:#}"), "on-demand camera stream failed");
        }
    })
}

async fn reverse_camera_stream(
    config: AgentConfig,
    stream_id: String,
    endpoint: crate::machine::BambuPrinterEndpoint,
    mode: CameraStreamMode,
) -> anyhow::Result<()> {
    let mut client = AgentControlClient::connect(config.hub_grpc_url.clone())
        .await
        .with_context(|| {
            format!(
                "connect on-demand camera stream to hub gRPC at {}",
                config.hub_grpc_url
            )
        })?;
    let (sender, receiver) = mpsc::channel(16);
    sender
        .send(camera_hello_event(&config))
        .await
        .context("queue agent camera hello event")?;
    let mut response = client
        .reverse_camera(Request::new(ReceiverStream::new(receiver)))
        .await
        .context("open reverse agent camera stream")?
        .into_inner();
    let mut task = spawn_camera_stream(config, stream_id.clone(), endpoint, mode, sender);

    loop {
        tokio::select! {
            result = &mut task => {
                result.context("join camera stream task")?;
                return Ok(());
            }
            command = response.next() => {
                let Some(command) = command
                    .transpose()
                    .context("read hub camera command from reverse stream")?
                else {
                    task.abort();
                    return Ok(());
                };
                if command.stream_id == stream_id
                    && matches!(command.command, Some(hub_camera_command::Command::Close(_)))
                {
                    task.abort();
                    return Ok(());
                }
            }
        }
    }
}

fn spawn_reverse_camera_closed(
    config: AgentConfig,
    stream_id: String,
    success: bool,
    error: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(err) = reverse_camera_closed(config, stream_id, success, error).await {
            tracing::warn!(error = %format!("{err:#}"), "on-demand camera close event failed");
        }
    })
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

fn spawn_camera_stream(
    config: AgentConfig,
    stream_id: String,
    endpoint: crate::machine::BambuPrinterEndpoint,
    mode: CameraStreamMode,
    sender: mpsc::Sender<AgentCameraEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (frame_sender, mut frame_receiver) = mpsc::channel(4);
        let mut worker = tokio::spawn(stream_camera(endpoint.clone(), mode, frame_sender));
        loop {
            tokio::select! {
                Some(frame) = frame_receiver.recv() => {
                    let event = camera_event(
                        &config,
                        &format!("camera-chunk-{stream_id}"),
                        agent_camera_event::Event::Chunk(AgentCameraChunk {
                            stream_id: stream_id.clone(),
                            data: frame,
                        }),
                    );
                    if sender.send(event).await.is_err() {
                        worker.abort();
                        break;
                    }
                }
                result = &mut worker => {
                    let closed = match result {
                        Ok(Ok(())) => AgentCameraClosed {
                            stream_id: stream_id.clone(),
                            success: true,
                            error: String::new(),
                        },
                        Ok(Err(err)) => AgentCameraClosed {
                            stream_id: stream_id.clone(),
                            success: false,
                            error: {
                                let error = crate::machine::diagnostics::redact_access_code(
                                    &format!("{err:#}"),
                                    &endpoint.access_code,
                                );
                                tracing::warn!(error = %error, "camera stream failed");
                                error
                            },
                        },
                        Err(err) if err.is_cancelled() => AgentCameraClosed {
                            stream_id: stream_id.clone(),
                            success: true,
                            error: String::new(),
                        },
                        Err(err) => AgentCameraClosed {
                            stream_id: stream_id.clone(),
                            success: false,
                            error: format!("{err:#}"),
                        },
                    };
                    let _ = sender
                        .send(camera_event(
                            &config,
                            &format!("camera-closed-{stream_id}"),
                            agent_camera_event::Event::Closed(closed),
                        ))
                        .await;
                    break;
                }
            }
        }
    })
}

async fn stream_camera(
    endpoint: crate::machine::BambuPrinterEndpoint,
    mode: CameraStreamMode,
    frame_sender: mpsc::Sender<Vec<u8>>,
) -> anyhow::Result<()> {
    match mode {
        CameraStreamMode::Unspecified | CameraStreamMode::Mjpeg => {
            stream_camera_mjpeg(endpoint, frame_sender).await
        }
        CameraStreamMode::FragmentedMp4 => {
            stream_camera_fragmented_mp4(endpoint, frame_sender).await
        }
    }
}

async fn send_camera_closed(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentCameraEvent>,
    stream_id: &str,
    success: bool,
    error: String,
) {
    let _ = sender
        .send(camera_event(
            config,
            &format!("camera-closed-{stream_id}"),
            agent_camera_event::Event::Closed(AgentCameraClosed {
                stream_id: stream_id.to_owned(),
                success,
                error,
            }),
        ))
        .await;
}

fn camera_event(
    config: &AgentConfig,
    event_id: &str,
    event: agent_camera_event::Event,
) -> AgentCameraEvent {
    AgentCameraEvent {
        agent_id: config.agent_id.to_string(),
        tenant_id: config.tenant_id.to_string(),
        event_id: event_id.to_owned(),
        event: Some(event),
    }
}
