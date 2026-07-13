use std::time::Duration;

use pandar_core::{FirmwareCommand, FirmwareTerminalOutcome};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::mpsc};

use super::*;
use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, FirmwareControlPhase, FirmwareObservationCache,
        mqtt::firmware_command_payload,
    },
};

#[tokio::test]
async fn rejected_ack_returns_typed_terminal_and_transient_status_without_cache_write() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
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
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"rejected","result":"fail","err_code":-7,"reason":"blocked","message":"rejected"},"print":{"cfg":"transient-only","upgrade_state":{"status":"failed","progress":"42"}}}"#,
        )
        .await;
        assert_eq!(read_packet(&mut stream).await.header >> 4, 14);
    });
    let cache = seeded_cache().await;
    let mut session = connect_session(address).await;
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();

    let outcome = complete_firmware_control_with_session(
        &mut session,
        firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "rejected".into(),
            src_id: 1,
        }),
        phases,
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        phase_receiver.recv().await,
        Some(FirmwareControlPhase::Published)
    );
    let FirmwareTerminalOutcome::Acknowledged { acknowledgement } = outcome.terminal else {
        panic!("expected typed rejected acknowledgement");
    };
    assert_eq!(acknowledgement.result.as_deref(), Some("fail"));
    assert_eq!(acknowledgement.error_code, Some(-7));
    assert_eq!(acknowledgement.reason.as_deref(), Some("blocked"));
    let transient = outcome.transient_status.unwrap();
    assert_eq!(transient.cfg.as_deref(), Some("transient-only"));
    assert_eq!(
        transient.upgrade_state.unwrap().progress.as_deref(),
        Some("42")
    );
    let snapshot = cache.snapshot("SERIAL").await.unwrap();
    assert_eq!(snapshot.status, None);
    assert_eq!(snapshot.status_revision, 0);
    broker.await.unwrap();
}

#[tokio::test]
async fn two_second_ack_timeout_returns_published_without_acknowledgement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
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
        match stream.read_u8().await {
            Ok(header) => assert_eq!(header >> 4, 14),
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
            )),
        }
    });
    let mut session = connect_session(address).await;
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();
    let started = tokio::time::Instant::now();

    let outcome = complete_firmware_control_with_session(
        &mut session,
        firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "timeout".into(),
            src_id: 1,
        }),
        phases,
        None,
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_secs(2));
    assert_eq!(
        phase_receiver.recv().await,
        Some(FirmwareControlPhase::Published)
    );
    assert_eq!(
        outcome.terminal,
        FirmwareTerminalOutcome::PublishedWithoutAcknowledgement
    );
    broker.await.unwrap();
}

async fn seeded_cache() -> FirmwareObservationCache {
    let cache = FirmwareObservationCache::default();
    let (sender, mut events) = mpsc::channel(2);
    let transition = cache
        .begin_generation(&test_config(), endpoint(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    drop(transition);
    events.recv().await.unwrap();
    cache
}

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "127.0.0.1".into(),
        serial: "SERIAL".into(),
        access_code: "secret".into(),
        model: Some("X1".into()),
        name: None,
    }
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
        artifact_root: ".".into(),
    }
}
