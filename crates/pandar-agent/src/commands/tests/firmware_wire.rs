use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use pandar_core::{FirmwareAcknowledgement, FirmwareTerminalOutcome};

use super::*;
use crate::machine::{
    FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest, FirmwareMachineGateway,
    FirmwareModulesDelivery, FirmwareModulesObservation, FirmwareObservationCache,
    FirmwarePrepareRequest, FirmwarePreparedObservation, FirmwareRefreshRequest,
    firmware_modules_event,
};
use pandar_protocol::agent::v1::{
    ExecuteFirmwareControl, FirmwareCommand, FirmwareUpgradeConfirm, PrepareFirmwareControl,
    RefreshFirmwareVersion, firmware_command, firmware_command_result, hub_command,
};

#[tokio::test]
async fn firmware_command_misroute_is_an_internal_worker_error() {
    let commands = [
        hub_command::Command::RefreshFirmwareVersion(RefreshFirmwareVersion {
            serial: "SERIAL".into(),
            sequence_id: "101".into(),
            expected_generation: 7,
        }),
        hub_command::Command::PrepareFirmwareControl(PrepareFirmwareControl {
            command_id: "outer-command".into(),
            serial: "SERIAL".into(),
            expected_generation: 7,
        }),
        hub_command::Command::ExecuteFirmwareControl(ExecuteFirmwareControl {
            command_id: "outer-command".into(),
            serial: "SERIAL".into(),
            expected_generation: 7,
            command: None,
        }),
    ];

    for command in commands {
        let (sender, _receiver) = mpsc::channel(1);
        let error = handle_command_with_gateway(
            &test_config(),
            &NoopMachineGateway,
            &sender,
            HubCommand {
                command_id: "outer-command".into(),
                command: Some(command),
            },
        )
        .await
        .unwrap_err();

        assert!(format!("{error:#}").contains("misrouted to non-firmware worker"));
    }
}

#[tokio::test]
async fn live_firmware_dispatcher_routes_refresh_prepare_and_execute_to_gateway() {
    let gateway = RecordingFirmwareGateway::default();
    let (sender, _receiver) = mpsc::channel(16);

    handle_firmware_command(
        &test_config(),
        &gateway,
        &sender,
        "refresh".into(),
        refresh_command(),
        81,
    )
    .await
    .unwrap();
    handle_firmware_command(
        &test_config(),
        &gateway,
        &sender,
        "prepare".into(),
        prepare_command("prepare"),
        81,
    )
    .await
    .unwrap();
    handle_firmware_command(
        &test_config(),
        &gateway,
        &sender,
        "execute".into(),
        execute_command("execute"),
        81,
    )
    .await
    .unwrap();

    assert_eq!(gateway.calls().await, ["refresh", "prepare", "execute"]);
}

#[tokio::test]
async fn firmware_immediate_execute_orders_ack_published_then_terminal_result() {
    let gateway = RecordingFirmwareGateway::default();
    let (sender, mut receiver) = mpsc::channel(4);

    handle_firmware_command(
        &test_config(),
        &gateway,
        &sender,
        "execute".into(),
        execute_command("execute"),
        81,
    )
    .await
    .unwrap();

    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    assert_eq!(
        events.len(),
        3,
        "published phase must precede terminal result"
    );
    assert!(matches!(
        events[0].event,
        Some(agent_event::Event::CommandAck(_))
    ));
    assert!(matches!(
        events[1].event,
        Some(agent_event::Event::FirmwarePublished(_))
    ));
    assert!(matches!(
        &events[2].event,
        Some(agent_event::Event::CommandResult(result)) if result.firmware_result.is_some()
    ));
}

#[tokio::test]
async fn fresh_refresh_result_is_enqueued_before_waiting_persistent_duplicate() {
    let config = test_config();
    let (sender, mut receiver) = mpsc::channel(1);
    let refresh_returned = Arc::new(tokio::sync::Notify::new());
    let duplicate_acquired = Arc::new(AtomicBool::new(false));
    let duplicate_notified = Arc::new(tokio::sync::Notify::new());
    let gateway = Arc::new(RefreshDeliveryOrderingGateway {
        cache: FirmwareObservationCache::default(),
        config: config.clone(),
        sender: sender.clone(),
        refresh_returned: Arc::clone(&refresh_returned),
        duplicate_acquired: Arc::clone(&duplicate_acquired),
        duplicate_notified: Arc::clone(&duplicate_notified),
    });

    let handler = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        let config = config.clone();
        let sender = sender.clone();
        async move {
            handle_firmware_command(
                &config,
                gateway.as_ref(),
                &sender,
                "refresh".into(),
                refresh_command(),
                81,
            )
            .await
        }
    });

    refresh_returned.notified().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), async {
            while !duplicate_acquired.load(Ordering::SeqCst) {
                duplicate_notified.notified().await;
            }
        })
        .await
        .is_err(),
        "typed refresh delivery must retain the version lease while its result send is blocked"
    );

    let ack = receiver.recv().await.unwrap();
    let fresh = receiver.recv().await.unwrap();
    let persistent = receiver.recv().await.unwrap();
    handler.await.unwrap().unwrap();
    assert!(matches!(ack.event, Some(agent_event::Event::CommandAck(_))));
    assert!(matches!(
        fresh.event,
        Some(agent_event::Event::CommandResult(result))
            if result
                .firmware_result
                .as_ref()
                .and_then(|result| result.outcome.as_ref())
                .is_some_and(|outcome| matches!(
                    outcome,
                    firmware_command_result::Outcome::RefreshedModules(modules)
                        if modules.module_revision == 1
                ))
    ));
    assert!(matches!(
        persistent.event,
        Some(agent_event::Event::PrinterFirmwareModulesSnapshot(snapshot))
            if snapshot.module_revision == 2
    ));
}

#[tokio::test]
async fn firmware_id_mismatch_is_rejected_before_gateway_dispatch() {
    for command in [prepare_command("inner"), execute_command("inner")] {
        let gateway = RecordingFirmwareGateway::default();
        let (sender, mut receiver) = mpsc::channel(4);

        let error = handle_firmware_command(
            &test_config(),
            &gateway,
            &sender,
            "outer".into(),
            command,
            82,
        )
        .await
        .unwrap_err();

        assert!(format!("{error:#}").contains("outer command id"));
        assert!(gateway.calls().await.is_empty());
        assert!(receiver.try_recv().is_err());
    }
}

fn refresh_command() -> hub_command::Command {
    hub_command::Command::RefreshFirmwareVersion(RefreshFirmwareVersion {
        serial: "SERIAL".into(),
        sequence_id: "101".into(),
        expected_generation: 7,
    })
}

fn prepare_command(command_id: &str) -> hub_command::Command {
    hub_command::Command::PrepareFirmwareControl(PrepareFirmwareControl {
        command_id: command_id.into(),
        serial: "SERIAL".into(),
        expected_generation: 7,
    })
}

fn execute_command(command_id: &str) -> hub_command::Command {
    hub_command::Command::ExecuteFirmwareControl(ExecuteFirmwareControl {
        command_id: command_id.into(),
        serial: "SERIAL".into(),
        expected_generation: 7,
        command: Some(FirmwareCommand {
            sequence_id: "102".into(),
            src_id: 1,
            command: Some(firmware_command::Command::UpgradeConfirm(
                FirmwareUpgradeConfirm {},
            )),
        }),
    })
}

#[derive(Clone, Default)]
struct RecordingFirmwareGateway {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

struct RefreshDeliveryOrderingGateway {
    cache: FirmwareObservationCache,
    config: AgentConfig,
    sender: mpsc::Sender<AgentEvent>,
    refresh_returned: Arc<tokio::sync::Notify>,
    duplicate_acquired: Arc<AtomicBool>,
    duplicate_notified: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl FirmwareMachineGateway for RefreshDeliveryOrderingGateway {
    async fn refresh_firmware_version(
        &self,
        request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        let lease = self.cache.version_observation_lease(&request.serial).await;
        let cache = self.cache.clone();
        let config = self.config.clone();
        let sender = self.sender.clone();
        let serial = request.serial.clone();
        let duplicate_acquired = Arc::clone(&self.duplicate_acquired);
        let duplicate_notified = Arc::clone(&self.duplicate_notified);
        tokio::spawn(async move {
            let _lease = cache.version_observation_lease(&serial).await;
            duplicate_acquired.store(true, Ordering::SeqCst);
            duplicate_notified.notify_one();
            sender
                .send(firmware_modules_event(
                    &config,
                    FirmwareModulesObservation {
                        serial,
                        generation: 7,
                        revision: 2,
                        modules: Vec::new(),
                    },
                ))
                .await
                .unwrap();
        });
        self.refresh_returned.notify_one();
        Ok(FirmwareModulesDelivery::with_version_observation_lease(
            FirmwareModulesObservation {
                serial: request.serial,
                generation: request.expected_generation,
                revision: 1,
                modules: Vec::new(),
            },
            lease,
        ))
    }

    async fn prepare_firmware_control(
        &self,
        _request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        unreachable!()
    }

    async fn execute_firmware_control(
        &self,
        _request: FirmwareExecuteRequest,
        _phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        unreachable!()
    }

    async fn cancel_firmware_session(&self, _session_epoch: u64) -> anyhow::Result<()> {
        Ok(())
    }
}

impl RecordingFirmwareGateway {
    async fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl FirmwareMachineGateway for RecordingFirmwareGateway {
    async fn refresh_firmware_version(
        &self,
        request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        self.calls.lock().await.push("refresh");
        Ok(FirmwareModulesDelivery::immediate(
            FirmwareModulesObservation {
                serial: request.serial,
                generation: request.expected_generation,
                revision: 1,
                modules: Vec::new(),
            },
        ))
    }

    async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        self.calls.lock().await.push("prepare");
        Ok(FirmwarePreparedObservation {
            command_id: request.command_id,
            serial: request.serial,
            generation: request.expected_generation,
        })
    }

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        self.calls.lock().await.push("execute");
        phases.send(FirmwareControlPhase::Published).unwrap();
        let (command, sequence_id) = match request.command {
            pandar_core::FirmwareCommand::UpgradeConfirm { sequence_id, .. } => {
                ("upgrade_confirm", sequence_id)
            }
            _ => unreachable!(),
        };
        Ok(FirmwareControlOutcome {
            terminal: FirmwareTerminalOutcome::Acknowledged {
                acknowledgement: FirmwareAcknowledgement {
                    command: command.into(),
                    sequence_id,
                    result: Some("success".into()),
                    error_code: None,
                    reason: None,
                    message: None,
                },
            },
            transient_status: None,
        })
    }

    async fn cancel_firmware_session(&self, _session_epoch: u64) -> anyhow::Result<()> {
        Ok(())
    }
}
