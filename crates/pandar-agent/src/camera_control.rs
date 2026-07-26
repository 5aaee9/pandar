use std::{collections::HashMap, future::Future};

use anyhow::Context;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
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

mod close;
#[cfg(test)]
mod tests;

use close::{report_reverse_camera_closed, spawn_reverse_camera_closed};

#[cfg(test)]
struct CameraJoinPause {
    reached: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
}

#[cfg(test)]
static CAMERA_JOIN_PAUSES: std::sync::OnceLock<std::sync::Mutex<HashMap<String, CameraJoinPause>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) struct CameraJoinPauseHandle {
    reached: tokio::sync::oneshot::Receiver<()>,
    _release: tokio::sync::oneshot::Sender<()>,
}

#[cfg(test)]
impl CameraJoinPauseHandle {
    pub(crate) async fn wait_reached(&mut self) {
        (&mut self.reached)
            .await
            .expect("camera join pause must be reached");
    }
}

#[cfg(test)]
pub(crate) fn install_camera_join_pause(stream_id: &str) -> CameraJoinPauseHandle {
    let (reached, reached_receiver) = tokio::sync::oneshot::channel();
    let (release, release_receiver) = tokio::sync::oneshot::channel();
    let previous = CAMERA_JOIN_PAUSES
        .get_or_init(Default::default)
        .lock()
        .expect("camera join pause lock must not be poisoned")
        .insert(
            stream_id.to_owned(),
            CameraJoinPause {
                reached,
                release: release_receiver,
            },
        );
    assert!(previous.is_none(), "camera join pause already installed");
    CameraJoinPauseHandle {
        reached: reached_receiver,
        _release: release,
    }
}

#[cfg(test)]
pub(crate) async fn pause_camera_join_for_test(stream_id: &str) {
    let pause = CAMERA_JOIN_PAUSES
        .get_or_init(Default::default)
        .lock()
        .expect("camera join pause lock must not be poisoned")
        .remove(stream_id);
    if let Some(pause) = pause {
        let _ = pause.reached.send(());
        let _ = pause.release.await;
    }
}

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
            streams.retain(|_, task| !task.is_finished());
            if !streams.contains_key(&command.stream_id) && streams.len() >= 4 {
                report_reverse_camera_closed(
                    config.clone(),
                    command.stream_id,
                    false,
                    "camera stream concurrency limit reached".to_owned(),
                )
                .await;
                return Ok(());
            }
            stop_camera_task(streams, &command.stream_id).await?;
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
            stop_camera_task(streams, &command.stream_id).await?;
        }
        None => {}
    }

    Ok(())
}

async fn stop_camera_task(
    streams: &mut HashMap<String, JoinHandle<()>>,
    stream_id: &str,
) -> anyhow::Result<()> {
    let Some(task) = streams.get_mut(stream_id) else {
        return Ok(());
    };
    task.abort();
    #[cfg(test)]
    pause_camera_join_for_test(stream_id).await;
    let result = (&mut *task).await;
    streams.remove(stream_id);
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(error).context("join replaced camera task"),
    }
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
    let response = client
        .reverse_camera(Request::new(ReceiverStream::new(receiver)))
        .await
        .context("open reverse agent camera stream")?
        .into_inner();
    let producer_endpoint = endpoint.clone();
    forward_reverse_camera_session(
        &config,
        &stream_id,
        &endpoint,
        sender,
        response,
        move |frame_sender| stream_camera(producer_endpoint, mode, frame_sender),
    )
    .await
}

async fn forward_reverse_camera_session<S, P, F>(
    config: &AgentConfig,
    stream_id: &str,
    endpoint: &crate::machine::BambuPrinterEndpoint,
    sender: mpsc::Sender<AgentCameraEvent>,
    mut response: S,
    producer: P,
) -> anyhow::Result<()>
where
    S: Stream<Item = Result<HubCameraCommand, tonic::Status>> + Unpin,
    P: FnOnce(mpsc::Sender<Vec<u8>>) -> F,
    F: Future<Output = anyhow::Result<()>>,
{
    let (frame_sender, frame_receiver) = mpsc::channel(4);
    let worker = producer(frame_sender);
    let forwarding =
        forward_camera_frames(config, stream_id, endpoint, sender, frame_receiver, worker);
    tokio::pin!(forwarding);

    loop {
        tokio::select! {
            () = &mut forwarding => return Ok(()),
            command = response.next() => {
                let Some(command) = command
                    .transpose()
                    .context("read hub camera command from reverse stream")?
                else {
                    return Ok(());
                };
                if command.stream_id == stream_id
                    && matches!(command.command, Some(hub_camera_command::Command::Close(_)))
                {
                    return Ok(());
                }
            }
        }
    }
}

async fn forward_camera_frames<F>(
    config: &AgentConfig,
    stream_id: &str,
    endpoint: &crate::machine::BambuPrinterEndpoint,
    sender: mpsc::Sender<AgentCameraEvent>,
    mut frame_receiver: mpsc::Receiver<Vec<u8>>,
    worker: F,
) where
    F: Future<Output = anyhow::Result<()>>,
{
    tokio::pin!(worker);
    let mut frames_open = true;
    loop {
        tokio::select! {
            frame = frame_receiver.recv(), if frames_open => {
                match frame {
                    Some(frame) => {
                    let event = camera_event(
                        config,
                        &format!("camera-chunk-{stream_id}"),
                        agent_camera_event::Event::Chunk(AgentCameraChunk {
                            stream_id: stream_id.to_owned(),
                            data: frame,
                        }),
                    );
                    if sender.send(event).await.is_err() {
                            return;
                        }
                    }
                    None => frames_open = false,
                }
            }
            result = &mut worker => {
                let closed = match result {
                        Ok(()) => AgentCameraClosed {
                            stream_id: stream_id.to_owned(),
                            success: true,
                            error: String::new(),
                        },
                        Err(err) => AgentCameraClosed {
                            stream_id: stream_id.to_owned(),
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
                };
                let _ = sender
                    .send(camera_event(
                        config,
                        &format!("camera-closed-{stream_id}"),
                        agent_camera_event::Event::Closed(closed),
                    ))
                    .await;
                return;
            }
        }
    }
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
