use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::{Notify, mpsc};
use tonic::{Request, Response, Status};

use super::*;
use crate::{
    machine::{
        BambuPrinterEndpoint,
        file_transfer::FakeMachineFileTransfer,
        mqtt::{BambuMqttTransport, PublishedMqttCommand},
        runtime::test_support::TestRuntimeBambuMachineGateway,
    },
    protocol::agent::v1::{
        AgentCameraEvent, AgentEvent, ExecuteFirmwareControl, FirmwareCommand,
        FirmwareUpgradeConfirm, HubCameraCommand, HubCommand, PrepareFirmwareControl,
        RefreshPrinters, agent_control_server::AgentControl, firmware_command, hub_command,
    },
};

pub(super) fn gateway(
    transport: BlockingMqttTransport,
) -> Arc<TestRuntimeBambuMachineGateway<BlockingMqttTransport, FakeMachineFileTransfer>> {
    let transfer = FakeMachineFileTransfer::default();
    Arc::new(TestRuntimeBambuMachineGateway::new(
        vec![(endpoint(), transport, transfer.clone())],
        transfer,
        Duration::from_secs(60),
    ))
}

pub(super) async fn seed_firmware_generation(
    gateway: &TestRuntimeBambuMachineGateway<BlockingMqttTransport, FakeMachineFileTransfer>,
) -> u64 {
    let (sender, mut events) = mpsc::channel(2);
    let transition = gateway
        .firmware_cache()
        .begin_generation(&test_config(), endpoint(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    events.recv().await.unwrap();
    generation
}

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".into(),
        serial: "SERIAL".into(),
        access_code: "secret".into(),
        model: Some("X1".into()),
        name: Some("office".into()),
    }
}

pub(super) fn runtime_endpoint(serial: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".into(),
        serial: serial.into(),
        access_code: "secret".into(),
        model: Some("X1".into()),
        name: Some(serial.into()),
    }
}

pub(super) fn refresh_command() -> HubCommand {
    HubCommand {
        command_id: "blocked-normal".into(),
        command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
    }
}

pub(super) fn prepare_command(outer: &str, inner: &str, generation: u64) -> HubCommand {
    HubCommand {
        command_id: outer.into(),
        command: Some(hub_command::Command::PrepareFirmwareControl(
            PrepareFirmwareControl {
                command_id: inner.into(),
                serial: "SERIAL".into(),
                expected_generation: generation,
            },
        )),
    }
}

pub(super) fn execute_command(command_id: &str) -> HubCommand {
    HubCommand {
        command_id: command_id.into(),
        command: Some(hub_command::Command::ExecuteFirmwareControl(
            ExecuteFirmwareControl {
                command_id: command_id.into(),
                serial: "SERIAL".into(),
                expected_generation: 1,
                command: Some(FirmwareCommand {
                    sequence_id: "late".into(),
                    src_id: 1,
                    command: Some(firmware_command::Command::UpgradeConfirm(
                        FirmwareUpgradeConfirm {},
                    )),
                }),
            },
        )),
    }
}

#[derive(Clone, Default)]
pub(super) struct BlockingMqttTransport {
    state: Arc<BlockingState>,
}

#[derive(Default)]
struct BlockingState {
    started: Notify,
    cancelled: AtomicBool,
}

impl BlockingMqttTransport {
    pub(super) async fn wait_until_blocked(&self) {
        self.state.started.notified().await;
    }

    pub(super) fn was_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }
}

struct CancellationMarker(Arc<BlockingState>);

impl Drop for CancellationMarker {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl BambuMqttTransport for BlockingMqttTransport {
    async fn subscribe(&self, _topic: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn publish(&self, _command: PublishedMqttCommand) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<serde_json::Value> {
        let _cancelled = CancellationMarker(Arc::clone(&self.state));
        self.state.started.notify_one();
        std::future::pending().await
    }
}

pub(super) struct CancellationAgentControlService {
    pub(super) connected: Arc<Notify>,
    pub(super) inbound_closed: Arc<Notify>,
}

pub(super) struct EofAgentControlService {
    pub(super) connected: Arc<Notify>,
    pub(super) inbound_closed: Arc<Notify>,
    pub(super) end_commands: Arc<Notify>,
}

pub(super) struct StatusAgentControlService {
    pub(super) connected: Arc<Notify>,
    pub(super) inbound_closed: Arc<Notify>,
    pub(super) fail_commands: Arc<Notify>,
}

#[tonic::async_trait]
impl AgentControl for CancellationAgentControlService {
    type ReverseConnectStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCommand, Status>> + Send>>;
    type ReverseCameraStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let mut inbound = request.into_inner();
        inbound.message().await?;
        self.connected.notify_one();
        let inbound_closed = Arc::clone(&self.inbound_closed);
        tokio::spawn(async move {
            loop {
                match inbound.message().await {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => {
                        inbound_closed.notify_one();
                        return;
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(tokio_stream::pending())))
    }

    async fn reverse_camera(
        &self,
        _request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        unreachable!()
    }
}

#[tonic::async_trait]
impl AgentControl for EofAgentControlService {
    type ReverseConnectStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCommand, Status>> + Send>>;
    type ReverseCameraStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let mut inbound = request.into_inner();
        inbound.message().await?;
        self.connected.notify_one();
        let inbound_closed = Arc::clone(&self.inbound_closed);
        tokio::spawn(async move {
            loop {
                match inbound.message().await {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => {
                        inbound_closed.notify_one();
                        return;
                    }
                }
            }
        });
        let (commands, receiver) = mpsc::channel(1);
        let end_commands = Arc::clone(&self.end_commands);
        tokio::spawn(async move {
            end_commands.notified().await;
            drop(commands);
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(receiver),
        )))
    }

    async fn reverse_camera(
        &self,
        _request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        unreachable!()
    }
}

#[tonic::async_trait]
impl AgentControl for StatusAgentControlService {
    type ReverseConnectStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCommand, Status>> + Send>>;
    type ReverseCameraStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<HubCameraCommand, Status>> + Send>>;

    async fn reverse_connect(
        &self,
        request: Request<tonic::Streaming<AgentEvent>>,
    ) -> Result<Response<Self::ReverseConnectStream>, Status> {
        let mut inbound = request.into_inner();
        inbound.message().await?;
        self.connected.notify_one();
        let inbound_closed = Arc::clone(&self.inbound_closed);
        tokio::spawn(async move {
            loop {
                match inbound.message().await {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => {
                        inbound_closed.notify_one();
                        return;
                    }
                }
            }
        });
        let (commands, receiver) = mpsc::channel(1);
        let fail_commands = Arc::clone(&self.fail_commands);
        tokio::spawn(async move {
            fail_commands.notified().await;
            let _ = commands
                .send(Err(Status::unavailable(
                    "firmware report stream status sentinel",
                )))
                .await;
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(receiver),
        )))
    }

    async fn reverse_camera(
        &self,
        _request: Request<tonic::Streaming<AgentCameraEvent>>,
    ) -> Result<Response<Self::ReverseCameraStream>, Status> {
        unreachable!()
    }
}
