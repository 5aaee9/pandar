use std::{
    collections::HashMap,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::body::Bytes;
use futures_util::Stream;
use pandar_core::AgentId;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use uuid::Uuid;

use crate::protocol::agent::v1::{
    CameraStreamMode, CloseCameraStream, HubCameraCommand, HubCommand, OpenCameraStream,
    hub_camera_command, hub_command,
};

#[derive(Debug, Clone)]
pub struct CameraSessionRegistry {
    streams: Arc<Mutex<HashMap<String, CameraStreamHandle>>>,
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
}

pub struct CameraHttpStream {
    stream_id: String,
    registry: CameraSessionRegistry,
    receiver: ReceiverStream<Result<Bytes, String>>,
}

impl CameraSessionRegistry {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn open_stream(
        &self,
        agent_id: AgentId,
        serial_number: String,
        command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    ) -> Result<CameraHttpStream, CameraOpenError> {
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

    pub async fn push_chunk(&self, stream_id: &str, data: Bytes) {
        let sender = self
            .streams
            .lock()
            .await
            .get(stream_id)
            .map(|handle| handle.sender.clone());
        if let Some(sender) = sender {
            let _ = sender.send(Ok(data)).await;
        }
    }

    pub async fn close_stream(&self, stream_id: &str, success: bool, error: String) {
        let sender = self
            .streams
            .lock()
            .await
            .remove(stream_id)
            .map(|handle| handle.sender);
        if let Some(sender) = sender
            && !success
        {
            let _ = sender.send(Err(error)).await;
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
mod tests {
    use futures_util::StreamExt;
    use pandar_core::AgentId;

    use super::*;

    #[tokio::test]
    async fn open_stream_sends_agent_command_and_forwards_chunks() {
        let registry = CameraSessionRegistry::new();
        let agent_id = AgentId::new();
        let (command_sender, mut command_receiver) = mpsc::channel(1);

        let mut stream = registry
            .open_stream(agent_id, "SERIAL-1".to_owned(), command_sender)
            .await
            .unwrap();
        let command = command_receiver.recv().await.unwrap().unwrap();
        assert_eq!(command.command_id, stream.stream_id);
        match command.command.unwrap() {
            hub_command::Command::CameraStream(command) => match command.command.unwrap() {
                hub_camera_command::Command::Open(open) => {
                    assert_eq!(command.stream_id, stream.stream_id);
                    assert_eq!(open.serial_number, "SERIAL-1");
                    assert_eq!(open.mode, CameraStreamMode::FragmentedMp4 as i32);
                }
                other => panic!("expected open camera command, got {other:?}"),
            },
            other => panic!("expected camera stream command, got {other:?}"),
        }

        registry
            .push_chunk(&stream.stream_id.clone(), Bytes::from_static(b"frame"))
            .await;
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk, Bytes::from_static(b"frame"));
    }

    #[tokio::test]
    async fn open_stream_replaces_existing_stream_for_same_printer() {
        let registry = CameraSessionRegistry::new();
        let agent_id = AgentId::new();
        let (command_sender, mut command_receiver) = mpsc::channel(4);

        let mut first = registry
            .open_stream(agent_id, "SERIAL-1".to_owned(), command_sender.clone())
            .await
            .unwrap();
        let first_open = camera_command(command_receiver.recv().await.unwrap().unwrap());

        let second = registry
            .open_stream(agent_id, "SERIAL-1".to_owned(), command_sender)
            .await
            .unwrap();
        let close = camera_command(command_receiver.recv().await.unwrap().unwrap());
        let second_open = camera_command(command_receiver.recv().await.unwrap().unwrap());

        assert_eq!(close.stream_id, first_open.stream_id);
        assert!(matches!(
            close.command,
            Some(hub_camera_command::Command::Close(_))
        ));
        assert_eq!(second_open.stream_id, second.stream_id);
        assert!(first.next().await.unwrap().is_err());
    }

    fn camera_command(command: HubCommand) -> HubCameraCommand {
        match command.command.unwrap() {
            hub_command::Command::CameraStream(command) => command,
            other => panic!("expected camera stream command, got {other:?}"),
        }
    }
}
