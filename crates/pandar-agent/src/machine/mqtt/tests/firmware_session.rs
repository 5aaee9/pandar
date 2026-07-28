use std::time::Duration;

use pandar_core::FirmwareCommand;
use rumqttc::MqttOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};

use super::super::firmware::{
    FirmwareMqttCommand, FirmwareMqttSession, firmware_barrier_pause, firmware_command_payload,
    is_firmware_post_publish_failure, is_firmware_pre_publish_failure,
    parse_firmware_acknowledgement,
};

pub(super) const REQUEST_TOPIC: &str = "device/SERIAL/request";
pub(super) const REPORT_TOPIC: &str = "device/SERIAL/report";

#[tokio::test]
async fn firmware_session_tcp_loopback_gates_matching_on_suback_and_own_publish() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (pre_report_sent, pre_report_received) = tokio::sync::oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_packet(&mut stream).await;
        assert_eq!(connect.header >> 4, 1);
        assert_ne!(
            connect.body[7] & 0x02,
            0,
            "CONNECT must request clean session"
        );
        let client_id = mqtt_string(&connect.body, 10).0;
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();

        let subscribe = read_packet(&mut stream).await;
        assert_eq!(subscribe.header >> 4, 8);
        let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        assert_eq!(mqtt_string(&subscribe.body, 2).0, REPORT_TOPIC);
        stream
            .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
            .await
            .unwrap();

        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"90001","result":"success"}}"#,
        )
        .await;
        pre_report_sent.send(()).unwrap();

        let publish = read_packet(&mut stream).await;
        assert_eq!(publish.header >> 4, 3);
        let (topic, topic_end) = mqtt_string(&publish.body, 0);
        assert_eq!(topic, REQUEST_TOPIC);
        let publish_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        assert_eq!(
            &publish.body[topic_end + 2..],
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"90001","src_id":1}}"#
        );
        stream
            .write_all(&[0x40, 0x02, (publish_id >> 8) as u8, publish_id as u8])
            .await
            .unwrap();

        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"consistency_confirm","sequence_id":"90001","result":"success"}}"#,
        )
        .await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"wrong","result":"success"}}"#,
        )
        .await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"90001","result":"success","err_code":0,"reason":"accepted","message":"queued"}}"#,
        )
        .await;

        let disconnect = read_packet(&mut stream).await;
        assert_eq!(disconnect.header >> 4, 14);
        client_id
    });

    let mut options = MqttOptions::new(
        "pandar-agent-fw-loopback-unique",
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(30)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    let mut session = FirmwareMqttSession::connect_with_options(
        options,
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
    )
    .await
    .unwrap();
    pre_report_received.await.unwrap();
    timeout(Duration::from_secs(1), async {
        while session.received_ordinal_for_test() < 1 {
            sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();

    let mut attempt = session
        .publish(firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "90001".into(),
            src_id: 1,
        }))
        .await
        .unwrap();
    attempt.wait_published().await.unwrap();
    let report = attempt
        .wait_matching_report(Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.ordinal, 4);
    let acknowledgement =
        parse_firmware_acknowledgement(&report.payload, "upgrade_confirm", "90001")
            .unwrap()
            .unwrap();
    assert_eq!(acknowledgement.result.as_deref(), Some("success"));
    assert_eq!(acknowledgement.error_code, Some(0));
    assert_eq!(acknowledgement.reason.as_deref(), Some("accepted"));
    assert_eq!(acknowledgement.message.as_deref(), Some("queued"));

    session.shutdown().await.unwrap();
    assert_eq!(broker.await.unwrap(), "pandar-agent-fw-loopback-unique");
}

#[tokio::test]
async fn firmware_session_barrier_and_publish_enqueue_are_pump_atomic() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (check_before_enqueue, check_receiver) = tokio::sync::oneshot::channel();
    let (checked, checked_receiver) = tokio::sync::oneshot::channel();
    let (publish_acked, publish_acked_receiver) = tokio::sync::oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        let subscribe = read_packet(&mut stream).await;
        let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        stream
            .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
            .await
            .unwrap();

        check_receiver.await.unwrap();
        assert!(
            timeout(Duration::from_millis(50), read_packet(&mut stream))
                .await
                .is_err(),
            "publish reached the broker while the pump was paused after its barrier"
        );
        checked.send(()).unwrap();

        let publish = read_packet(&mut stream).await;
        assert_eq!(publish.header >> 4, 3);
        let (_, topic_end) = mqtt_string(&publish.body, 0);
        let publish_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        stream
            .write_all(&[0x40, 0x02, (publish_id >> 8) as u8, publish_id as u8])
            .await
            .unwrap();
        publish_acked.send(()).unwrap();
        assert_disconnect_or_reset(&mut stream).await;
    });

    let mut options = MqttOptions::new(
        "pandar-agent-fw-barrier",
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    let (barrier_pause, mut barrier_control) = firmware_barrier_pause();
    let mut session = FirmwareMqttSession::connect_with_options_and_barrier_pause(
        options,
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        barrier_pause,
    )
    .await
    .unwrap();
    let mut attempt = session
        .publish(firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "atomic".into(),
            src_id: 1,
        }))
        .await
        .unwrap();
    barrier_control.wait_until_reached().await;
    check_before_enqueue.send(()).unwrap();
    checked_receiver.await.unwrap();
    barrier_control.release();
    attempt.wait_published().await.unwrap();
    publish_acked_receiver.await.unwrap();
    session.shutdown().await.unwrap();
    broker.await.unwrap();
}

#[tokio::test]
async fn firmware_session_barrier_cancellation_is_pre_publish_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        let subscribe = read_packet(&mut stream).await;
        let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        stream
            .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
            .await
            .unwrap();
        assert_disconnect_or_reset(&mut stream).await;
    });

    let mut options = MqttOptions::new(
        "pandar-agent-fw-pre-publish",
        (address.ip().to_string(), address.port()),
    );
    options.set_clean_session(true);
    let (barrier_pause, mut barrier_control) = firmware_barrier_pause();
    let mut session = FirmwareMqttSession::connect_with_options_and_barrier_pause(
        options,
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        barrier_pause,
    )
    .await
    .unwrap();
    let mut attempt = session
        .publish(firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "cancel-before-publish".into(),
            src_id: 1,
        }))
        .await
        .unwrap();
    barrier_control.wait_until_reached().await;
    barrier_control.cancel();
    let error = attempt.wait_published().await.unwrap_err();
    assert!(is_firmware_pre_publish_failure(&error));
    session.shutdown().await.unwrap();
    broker.await.unwrap();
}

#[tokio::test]
async fn malformed_report_after_own_publish_preserves_outcome_unknown_phase() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        let subscribe = read_packet(&mut stream).await;
        let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        stream
            .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
            .await
            .unwrap();
        let publish = read_packet(&mut stream).await;
        let (_, topic_end) = mqtt_string(&publish.body, 0);
        let publish_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        stream
            .write_all(&[0x40, 0x02, (publish_id >> 8) as u8, publish_id as u8])
            .await
            .unwrap();
        write_publish(&mut stream, REPORT_TOPIC, br#"{"#).await;
        let closed = timeout(Duration::from_secs(1), stream.read_u8())
            .await
            .expect("malformed report must end the firmware pump");
        assert!(
            closed.is_err(),
            "firmware pump unexpectedly wrote packet {closed:?}"
        );
    });
    let mut options = MqttOptions::new(
        "pandar-agent-fw-malformed-post-publish",
        (address.ip().to_string(), address.port()),
    );
    options.set_clean_session(true);
    let mut session = FirmwareMqttSession::connect_with_options(
        options,
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
    )
    .await
    .unwrap();
    let mut attempt = session
        .publish(FirmwareMqttCommand::get_version("malformed"))
        .await
        .unwrap();
    attempt.wait_published().await.unwrap();

    let error = attempt
        .wait_matching_report(Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(is_firmware_post_publish_failure(&error), "{error:#}");
    let _ = session.shutdown().await;
    broker.await.unwrap();
}

pub(super) struct Packet {
    pub(super) header: u8,
    pub(super) body: Vec<u8>,
}

pub(super) async fn read_packet(stream: &mut TcpStream) -> Packet {
    let header = stream.read_u8().await.unwrap();
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

pub(super) fn mqtt_string(body: &[u8], offset: usize) -> (String, usize) {
    let length = usize::from(u16::from_be_bytes([body[offset], body[offset + 1]]));
    let start = offset + 2;
    let end = start + length;
    (String::from_utf8(body[start..end].to_vec()).unwrap(), end)
}

pub(super) async fn write_publish(stream: &mut TcpStream, topic: &str, payload: &[u8]) {
    let mut body = Vec::with_capacity(topic.len() + payload.len() + 2);
    body.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    body.extend_from_slice(topic.as_bytes());
    body.extend_from_slice(payload);
    let mut packet = vec![0x30];
    encode_remaining_length(body.len(), &mut packet);
    packet.extend_from_slice(&body);
    stream.write_all(&packet).await.unwrap();
}

async fn assert_disconnect_or_reset(stream: &mut TcpStream) {
    match stream.read_u8().await {
        Ok(header) => assert_eq!(header >> 4, 14),
        Err(error) => assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
            ),
            "unexpected firmware session close error: {error}"
        ),
    }
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
