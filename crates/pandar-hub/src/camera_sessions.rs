use std::{
    collections::HashMap,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::body::Bytes;
use futures_util::Stream;
use pandar_core::{AgentId, TenantId};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use uuid::Uuid;

use crate::protocol::agent::v1::{
    CameraStreamMode, CloseCameraStream, HubCameraCommand, HubCommand, OpenCameraStream,
    hub_camera_command, hub_command,
};

mod capacity;

use capacity::{CameraCapacity, CameraCapacityPermit};

#[derive(Debug, Clone)]
pub struct CameraSessionRegistry {
    streams: Arc<Mutex<HashMap<String, CameraStreamHandle>>>,
    capacity: Arc<CameraCapacity>,
}

#[derive(Debug, Clone)]
struct CameraStreamHandle {
    agent_id: AgentId,
    serial_number: String,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    sender: mpsc::Sender<Result<Bytes, String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraOpenError {
    AgentOffline,
    ChannelClosed,
    ChannelFull,
    Capacity,
}

pub struct CameraHttpStream {
    stream_id: String,
    registry: CameraSessionRegistry,
    receiver: ReceiverStream<Result<Bytes, String>>,
    _capacity_permit: CameraCapacityPermit,
}

pub(crate) const MAX_CAMERA_CHUNK_BYTES: usize = 64 * 1024;

impl CameraSessionRegistry {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
            capacity: Arc::new(CameraCapacity::new()),
        }
    }

    pub async fn open_stream(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        serial_number: String,
        command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    ) -> Result<CameraHttpStream, CameraOpenError> {
        let capacity_permit = self.capacity.acquire(tenant_id)?;
        let stream_id = Uuid::new_v4().to_string();
        let (sender, receiver) = mpsc::channel(16);
        let replaced_stream_ids = {
            let mut streams = self.streams.lock().await;
            let replaced_stream_ids = streams
                .iter()
                .filter(|(_, handle)| {
                    handle.agent_id == agent_id && handle.serial_number == serial_number
                })
                .map(|(stream_id, handle)| (stream_id.clone(), handle.command_sender.clone()))
                .collect::<Vec<_>>();
            for (replaced_stream_id, _) in &replaced_stream_ids {
                if let Some(handle) = streams.remove(replaced_stream_id) {
                    let _ = handle.sender.try_send(Err("camera_replaced".to_owned()));
                }
            }
            streams.insert(
                stream_id.clone(),
                CameraStreamHandle {
                    agent_id,
                    serial_number: serial_number.clone(),
                    command_sender: command_sender.clone(),
                    sender,
                },
            );
            replaced_stream_ids
        };
        for (replaced_stream_id, sender) in replaced_stream_ids {
            let _ = sender.try_send(Ok(camera_control_command(HubCameraCommand {
                stream_id: replaced_stream_id,
                command: Some(hub_camera_command::Command::Close(CloseCameraStream {})),
            })));
        }
        let command = HubCameraCommand {
            stream_id: stream_id.clone(),
            command: Some(hub_camera_command::Command::Open(OpenCameraStream {
                serial_number,
                mode: CameraStreamMode::FragmentedMp4 as i32,
            })),
        };

        match command_sender.try_send(Ok(camera_control_command(command))) {
            Ok(()) => Ok(CameraHttpStream {
                stream_id,
                registry: self.clone(),
                receiver: ReceiverStream::new(receiver),
                _capacity_permit: capacity_permit,
            }),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.streams.lock().await.remove(&stream_id);
                Err(CameraOpenError::ChannelClosed)
            }

            Err(mpsc::error::TrySendError::Full(_)) => {
                self.streams.lock().await.remove(&stream_id);
                Err(CameraOpenError::ChannelFull)
            }
        }
    }

    pub async fn push_chunk(&self, agent_id: AgentId, stream_id: &str, data: Bytes) {
        let sender = self
            .streams
            .lock()
            .await
            .get(stream_id)
            .filter(|handle| handle.agent_id == agent_id)
            .map(|handle| handle.sender.clone());
        if let Some(sender) = sender
            && sender.try_send(Ok(data)).is_err()
        {
            let mut streams = self.streams.lock().await;
            if streams
                .get(stream_id)
                .is_some_and(|handle| handle.agent_id == agent_id)
            {
                streams.remove(stream_id);
            }
        }
    }

    pub async fn close_stream(
        &self,
        agent_id: AgentId,
        stream_id: &str,
        success: bool,
        error: String,
    ) {
        let sender = {
            let mut streams = self.streams.lock().await;
            let owned = streams
                .get(stream_id)
                .is_some_and(|handle| handle.agent_id == agent_id);
            owned
                .then(|| streams.remove(stream_id).map(|handle| handle.sender))
                .flatten()
        };
        if let Some(sender) = sender
            && !success
        {
            let _ = sender.try_send(Err(error));
        }
    }

    pub async fn close_agent(&self, agent_id: AgentId) {
        let closed = {
            let mut streams = self.streams.lock().await;
            let stream_ids = streams
                .iter()
                .filter(|(_, handle)| handle.agent_id == agent_id)
                .map(|(stream_id, _)| stream_id.clone())
                .collect::<Vec<_>>();
            stream_ids
                .into_iter()
                .filter_map(|stream_id| {
                    streams
                        .remove(&stream_id)
                        .map(|handle| (stream_id, handle.command_sender, handle.sender))
                })
                .collect::<Vec<_>>()
        };
        for (stream_id, command_sender, sender) in closed {
            let _ = sender.try_send(Err("agent_session_closed".to_owned()));
            let _ = command_sender.try_send(Ok(camera_control_command(HubCameraCommand {
                stream_id,
                command: Some(hub_camera_command::Command::Close(CloseCameraStream {})),
            })));
        }
    }

    async fn abort_stream(&self, stream_id: String) {
        let command_sender = self
            .streams
            .lock()
            .await
            .remove(&stream_id)
            .map(|handle| handle.command_sender);
        if let Some(command_sender) = command_sender {
            let _ = command_sender.try_send(Ok(camera_control_command(HubCameraCommand {
                stream_id,
                command: Some(hub_camera_command::Command::Close(CloseCameraStream {})),
            })));
        }
    }
}

fn camera_control_command(command: HubCameraCommand) -> HubCommand {
    HubCommand {
        command_id: command.stream_id.clone(),
        command: Some(hub_command::Command::CameraStream(command)),
    }
}

impl Stream for CameraHttpStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Pin::new(&mut this.receiver).poll_next(cx).map(|item| {
            item.map(|result| {
                result.map_err(|error| io::Error::new(io::ErrorKind::ConnectionAborted, error))
            })
        })
    }
}

impl Drop for CameraHttpStream {
    fn drop(&mut self) {
        let registry = self.registry.clone();
        let stream_id = self.stream_id.clone();
        tokio::spawn(async move {
            registry.abort_stream(stream_id).await;
        });
    }
}

impl Default for CameraSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
