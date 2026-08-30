use async_trait::async_trait;
use pandar_core::FirmwareCommand as CoreFirmwareCommand;
use tokio::{net::TcpListener, sync::mpsc};

use super::*;
use crate::{
    AgentConfig,
    commands::handle_firmware_command,
    machine::{
        FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest,
        FirmwareMachineGateway, FirmwareModulesDelivery, FirmwarePrepareRequest,
        FirmwarePreparedObservation, FirmwareRefreshRequest, mqtt::firmware_command_payload,
    },
};
use pandar_protocol::agent::v1::{
    ExecuteFirmwareControl, FirmwareCommand, FirmwareStart, agent_event, firmware_command,
    firmware_command_result, hub_command,
};

const URL_SENTINEL: &str =
    "https://user:password@example.invalid/firmware.bin?sig=UNIQUE-URL-SENTINEL";
const NONMATCHING_URL: &str = "https://user:password@example.invalid/other.bin?sig=NONMATCHING";

#[tokio::test]
async fn printer_echoed_pending_url_is_redacted_from_every_typed_outcome_text_field() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let report = serde_json::json!({
        "upgrade": {
            "command": "start",
            "sequence_id": "redacted-outcome",
            "result": echoed("ack-result"),
            "reason": echoed("ack-reason"),
            "message": echoed("ack-message")
        },
        "print": {
            "cfg": echoed("cfg"),
            "upgrade_state": {
                "status": echoed("upgrade-status"),
                "progress": echoed("upgrade-progress"),
                "message": echoed("upgrade-message"),
                "module": echoed("upgrade-module"),
                "ota_new_version_number": echoed("ota-new-version"),
                "ams_new_version_number": echoed("ams-new-version"),
                "ahb_new_version_number": echoed("ahb-new-version"),
                "new_ver_list": [{
                    "name": echoed("new-version-name"),
                    "cur_ver": echoed("current-version"),
                    "new_ver": echoed("new-version")
                }],
                "mc_for_ams_firmware": {
                    "status": echoed("ams-switch-status"),
                    "firmware": [{
                        "id": 7,
                        "name": echoed("ams-firmware-name"),
                        "version": echoed("ams-firmware-version")
                    }]
                }
            }
        }
    });
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        let publish = read_packet(&mut stream).await;
        let (_, topic_end) = mqtt_string(&publish.body, 0);
        let packet_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        stream
            .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
            .await
            .unwrap();
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            &serde_json::to_vec(&report).unwrap(),
        )
        .await;
        assert_eq!(read_packet(&mut stream).await.header >> 4, 14);
    });
    let mut session = connect_session(address).await;
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();

    let outcome = complete_firmware_control_with_session(
        &mut session,
        firmware_command_payload(&CoreFirmwareCommand::Start {
            sequence_id: "redacted-outcome".into(),
            src_id: 1,
            url: URL_SENTINEL.into(),
            module: "ota".into(),
            version: "01.02.03.04".into(),
        }),
        phases,
        Some(URL_SENTINEL),
    )
    .await
    .unwrap();

    assert_eq!(
        phase_receiver.recv().await,
        Some(FirmwareControlPhase::Published)
    );
    assert!(!format!("{outcome:?}").contains(URL_SENTINEL));
    let FirmwareControlOutcome {
        terminal: FirmwareTerminalOutcome::Acknowledged { acknowledgement },
        transient_status: Some(transient),
    } = outcome
    else {
        panic!("expected acknowledged outcome with transient status");
    };
    assert_redacted(acknowledgement.result.as_deref(), "ack-result");
    assert_redacted(acknowledgement.reason.as_deref(), "ack-reason");
    assert_redacted(acknowledgement.message.as_deref(), "ack-message");
    assert_redacted(transient.cfg.as_deref(), "cfg");
    let upgrade = transient.upgrade_state.unwrap();
    assert_redacted(upgrade.status.as_deref(), "upgrade-status");
    assert_redacted(upgrade.progress.as_deref(), "upgrade-progress");
    assert_redacted(upgrade.message.as_deref(), "upgrade-message");
    assert_redacted(upgrade.module.as_deref(), "upgrade-module");
    assert_redacted(upgrade.ota_new_version_number.as_deref(), "ota-new-version");
    assert_redacted(upgrade.ams_new_version_number.as_deref(), "ams-new-version");
    assert_redacted(upgrade.ahb_new_version_number.as_deref(), "ahb-new-version");
    let version = &upgrade.new_versions.as_ref().unwrap()[0];
    assert_eq!(version.name, redacted("new-version-name"));
    assert_redacted(version.current_version.as_deref(), "current-version");
    assert_redacted(version.new_version.as_deref(), "new-version");
    let ams = upgrade.ams_firmware.as_ref().unwrap();
    assert_redacted(ams.status.as_deref(), "ams-switch-status");
    let descriptor = &ams.firmware.as_ref().unwrap()[0];
    assert_eq!(descriptor.name, redacted("ams-firmware-name"));
    assert_eq!(descriptor.version, redacted("ams-firmware-version"));
    broker.await.unwrap();
}

fn echoed(field: &str) -> String {
    format!("{field}\r\n<{URL_SENTINEL}>|{NONMATCHING_URL}|tail")
}

fn redacted(field: &str) -> String {
    format!("{field}\r\n<[redacted]>|{NONMATCHING_URL}|tail")
}

fn assert_redacted(actual: Option<&str>, field: &str) {
    let expected = redacted(field);
    assert_eq!(actual, Some(expected.as_str()));
}

#[test]
fn start_url_is_published_once_and_absent_from_events_errors_and_tracing() {
    let (logs, (payload, events, result_debug)) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let broker = tokio::spawn(async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    accept_subscription(&mut stream).await;
                    let publish = read_packet(&mut stream).await;
                    assert_eq!(publish.header >> 4, 3);
                    let (_, topic_end) = mqtt_string(&publish.body, 0);
                    let packet_id =
                        u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
                    stream
                        .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
                        .await
                        .unwrap();
                    let payload = publish.body[topic_end + 2..].to_vec();
                    drop(stream);
                    payload
                });
                let session = connect_session(address).await;
                let gateway = LoopbackExecuteGateway {
                    session: tokio::sync::Mutex::new(Some(session)),
                };
                let (sender, mut receiver) = mpsc::channel(8);
                let result = handle_firmware_command(
                    &test_config(),
                    &gateway,
                    &sender,
                    "start-command".into(),
                    start_command(),
                    120,
                )
                .await;
                let mut events = Vec::new();
                while let Ok(event) = receiver.try_recv() {
                    events.push(event);
                }
                (broker.await.unwrap(), events, format!("{result:?}"))
            })
    });

    let serialized = String::from_utf8(payload.clone()).unwrap();
    assert_eq!(serialized.matches(URL_SENTINEL).count(), 1);
    let payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(payload["upgrade"]["command"], "start");
    assert_eq!(payload["upgrade"]["url"], URL_SENTINEL);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.event, Some(agent_event::Event::FirmwarePublished(_))))
            .count(),
        1
    );
    assert!(events.iter().any(|event| matches!(
        &event.event,
        Some(agent_event::Event::CommandResult(result))
            if matches!(
                result
                    .firmware_result
                    .as_ref()
                    .and_then(|firmware| firmware.outcome.as_ref()),
                Some(firmware_command_result::Outcome::PublishedWithoutAcknowledgement(_))
            )
    )));
    assert!(!format!("{events:?}").contains(URL_SENTINEL));
    assert!(!result_debug.contains(URL_SENTINEL));
    let logs = logs.contents();
    assert!(logs.contains("outcome unknown"));
    assert!(!logs.contains(URL_SENTINEL));
}

struct LoopbackExecuteGateway {
    session: tokio::sync::Mutex<Option<FirmwareMqttSession>>,
}

#[async_trait]
impl FirmwareMachineGateway for LoopbackExecuteGateway {
    async fn refresh_firmware_version(
        &self,
        _request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        unreachable!()
    }

    async fn prepare_firmware_control(
        &self,
        _request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        unreachable!()
    }

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        let pending_url = match &request.command {
            CoreFirmwareCommand::Start { url, .. } => Some(url.as_str()),
            _ => unreachable!(),
        };
        let command = firmware_command_payload(&request.command);
        let mut session = self.session.lock().await.take().unwrap();
        complete_firmware_control_with_session(&mut session, command, phases, pending_url).await
    }

    async fn cancel_firmware_session(&self, _session_epoch: u64) -> anyhow::Result<()> {
        Ok(())
    }
}

fn start_command() -> hub_command::Command {
    hub_command::Command::ExecuteFirmwareControl(ExecuteFirmwareControl {
        command_id: "start-command".into(),
        serial: "SERIAL".into(),
        expected_generation: 1,
        command: Some(FirmwareCommand {
            sequence_id: "start-sequence".into(),
            src_id: 1,
            command: Some(firmware_command::Command::Start(FirmwareStart {
                url: URL_SENTINEL.into(),
                module: "ota".into(),
                version: "01.02.03.04".into(),
            })),
        }),
    })
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.invalid".into(),
        hub_api_url: None,
        agent_name: "test".into(),
        agent_id: "agent".into(),
        tenant_id: "tenant".into(),
        agent_credential: "credential".into(),
        agent_version: "test".into(),
        printers: "[]".into(),
    }
}
