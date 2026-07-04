use std::collections::HashMap;

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::{Stream, StreamExt};
use tonic::Status;

use crate::{
    AgentConfig,
    machine::{
        BambuMachineGateway,
        camera::{stream_camera_fragmented_mp4, stream_camera_mjpeg},
    },
    protocol::agent::v1::{
        AgentCameraChunk, AgentCameraClosed, AgentCameraEvent, AgentCameraHello, CameraStreamMode,
        HubCameraCommand, agent_camera_event, hub_camera_command,
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

pub async fn handle_camera_command_stream_with_gateway<G, S>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentCameraEvent>,
    mut commands: S,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    S: Stream<Item = Result<HubCameraCommand, Status>> + Unpin,
{
    let mut streams: HashMap<String, JoinHandle<()>> = HashMap::new();
    while let Some(command) = commands
        .next()
        .await
        .transpose()
        .context("read hub camera command from reverse stream")?
    {
        match command.command {
            Some(hub_camera_command::Command::Open(open)) => {
                if let Some(task) = streams.remove(&command.stream_id) {
                    task.abort();
                }
                let stream_id = command.stream_id.clone();
                let endpoint = match gateway.camera_endpoint(&open.serial_number).await {
                    Ok(endpoint) => endpoint,
                    Err(err) => {
                        let error = gateway.redact_error(&format!("{err:#}"));
                        send_camera_closed(config, sender, &stream_id, false, error).await;
                        continue;
                    }
                };
                let task = spawn_camera_stream(
                    config.clone(),
                    stream_id.clone(),
                    endpoint,
                    CameraStreamMode::try_from(open.mode).unwrap_or(CameraStreamMode::Mjpeg),
                    sender.clone(),
                );
                streams.insert(stream_id, task);
            }
            Some(hub_camera_command::Command::Close(_)) => {
                if let Some(task) = streams.remove(&command.stream_id) {
                    task.abort();
                }
            }
            None => {}
        }
    }

    for (_, task) in streams {
        task.abort();
    }
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
