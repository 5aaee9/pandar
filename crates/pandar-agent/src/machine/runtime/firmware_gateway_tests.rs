use std::time::Duration;

use pandar_core::{FirmwareAcknowledgement, FirmwareTerminalOutcome};
use rumqttc::MqttOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    time::timeout,
};

use crate::machine::{
    FirmwareControlPhase,
    mqtt::{
        FirmwareMqttCommand, FirmwareMqttSession, FirmwareMqttTaskSet, firmware_barrier_pause,
        is_firmware_pre_publish_failure,
    },
};

use super::firmware_gateway::{complete_firmware_control_with_session, redact_pending_url};

mod outcomes;
mod pump_cleanup;
mod pump_registration;
mod url_redaction;

const REQUEST_TOPIC: &str = "device/SERIAL/request";
const REPORT_TOPIC: &str = "device/SERIAL/report";

#[tokio::test]
async fn control_disconnect_before_own_publish_is_safe_failure_without_published_phase() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (disconnect, wait_disconnect) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        wait_disconnect.await.unwrap();
        drop(stream);
    });
    let mut session = connect_session(address).await;
    disconnect.send(()).unwrap();
    broker.await.unwrap();
    timeout(Duration::from_secs(1), async {
        while !session.pump_finished_for_test() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();

    let result = complete_firmware_control_with_session(
        &mut session,
        FirmwareMqttCommand::get_version("before-publish"),
        phases,
        None,
    )
    .await;

    let sentinel = "https://example.invalid/UNIQUE-URL-SENTINEL";
    let error = result
        .expect_err("a pre-publish disconnect must be safe to fail")
        .context(format!("prepare firmware URL {sentinel}"));
    let error = redact_pending_url(error, Some(sentinel));
    assert!(is_firmware_pre_publish_failure(&error), "{error:#}");
    assert!(!format!("{error:#}").contains(sentinel));
    assert!(phase_receiver.try_recv().is_err());
}

#[tokio::test]
async fn control_disconnect_after_own_publish_emits_phase_and_returns_outcome_unknown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        let publish = read_packet(&mut stream).await;
        assert_eq!(publish.header >> 4, 3);
        drop(stream);
    });
    let mut session = connect_session(address).await;
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();

    let outcome = complete_firmware_control_with_session(
        &mut session,
        FirmwareMqttCommand::get_version("after-publish"),
        phases,
        None,
    )
    .await
    .unwrap();

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

#[tokio::test]
#[expect(
    deprecated,
    reason = "SO_LINGER zero is required for an abortive TCP reset"
)]
async fn acknowledged_control_survives_broker_reset_during_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (reset, wait_for_reset) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        let publish = read_packet(&mut stream).await;
        assert_eq!(publish.header >> 4, 3);
        let (_, topic_end) = mqtt_string(&publish.body, 0);
        let packet_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        stream
            .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
            .await
            .unwrap();
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"info":{"command":"get_version","sequence_id":"ack-then-reset","module":[{"name":"ota","product_name":"X1","sw_ver":"1"}]},"upgrade":{"command":"get_version","sequence_id":"ack-then-reset","result":"success"}}"#,
        )
        .await;
        wait_for_reset.await.unwrap();
        stream.set_linger(Some(Duration::ZERO)).unwrap();
        drop(stream);
    });
    let mut session = connect_session(address).await;
    let mut shutdown_pause = session.pause_shutdown_for_test();
    let pump_finished = session.pump_finished_flag_for_test();
    let (phases, mut phase_receiver) = mpsc::unbounded_channel();

    let (outcome, ()) = tokio::join!(
        complete_firmware_control_with_session(
            &mut session,
            FirmwareMqttCommand::get_version("ack-then-reset"),
            phases,
            None,
        ),
        async {
            shutdown_pause.wait_until_reached().await;
            reset.send(()).unwrap();
            broker.await.unwrap();
            timeout(Duration::from_secs(1), async {
                while !pump_finished.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            shutdown_pause.release();
        }
    );
    let outcome = outcome.unwrap();

    assert_eq!(
        phase_receiver.recv().await,
        Some(FirmwareControlPhase::Published)
    );
    assert_eq!(
        outcome.terminal,
        FirmwareTerminalOutcome::Acknowledged {
            acknowledgement: FirmwareAcknowledgement {
                command: "get_version".into(),
                sequence_id: "ack-then-reset".into(),
                result: Some("success".into()),
                error_code: None,
                reason: None,
                message: None,
            }
        }
    );
}

async fn connect_session(address: std::net::SocketAddr) -> FirmwareMqttSession {
    let mut options = MqttOptions::new(
        format!("firmware-runtime-test-{}", uuid::Uuid::new_v4()),
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(30)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    FirmwareMqttSession::connect_with_options(options, REQUEST_TOPIC.into(), REPORT_TOPIC.into())
        .await
        .unwrap()
}

async fn accept_subscription(stream: &mut TcpStream) {
    assert_eq!(read_packet(stream).await.header >> 4, 1);
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    let subscribe = read_packet(stream).await;
    assert_eq!(subscribe.header >> 4, 8);
    let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
    stream
        .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
        .await
        .unwrap();
}

struct Packet {
    header: u8,
    body: Vec<u8>,
}

async fn read_packet(stream: &mut TcpStream) -> Packet {
    let header = timeout(Duration::from_secs(1), stream.read_u8())
        .await
        .unwrap()
        .unwrap();
    let mut multiplier = 1usize;
    let mut remaining = 0usize;
    loop {
        let encoded = stream.read_u8().await.unwrap();
        remaining += usize::from(encoded & 0x7f) * multiplier;
        if encoded & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let mut body = vec![0; remaining];
    stream.read_exact(&mut body).await.unwrap();
    Packet { header, body }
}

fn mqtt_string(body: &[u8], offset: usize) -> (String, usize) {
    let length = usize::from(u16::from_be_bytes([body[offset], body[offset + 1]]));
    let start = offset + 2;
    let end = start + length;
    (String::from_utf8(body[start..end].to_vec()).unwrap(), end)
}

async fn write_publish(stream: &mut TcpStream, topic: &str, payload: &[u8]) {
    let mut body = Vec::with_capacity(topic.len() + payload.len() + 2);
    body.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    body.extend_from_slice(topic.as_bytes());
    body.extend_from_slice(payload);
    let mut packet = vec![0x30];
    encode_remaining_length(body.len(), &mut packet);
    packet.extend_from_slice(&body);
    stream.write_all(&packet).await.unwrap();
}

fn encode_remaining_length(mut length: usize, output: &mut Vec<u8>) {
    loop {
        let mut encoded = (length % 128) as u8;
        length /= 128;
        if length > 0 {
            encoded |= 0x80;
        }
        output.push(encoded);
        if length == 0 {
            return;
        }
    }
}
