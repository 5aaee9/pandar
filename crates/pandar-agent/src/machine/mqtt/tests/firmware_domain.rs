use std::time::Duration;

use pandar_core::FirmwareCommand;
use rumqttc::MqttOptions;
use tokio::{io::AsyncWriteExt, net::TcpListener};

use super::firmware_session::{
    REPORT_TOPIC, REQUEST_TOPIC, mqtt_string, read_packet, write_publish,
};
use crate::machine::mqtt::{FirmwareMqttCommand, FirmwareMqttSession, firmware_command_payload};

#[tokio::test]
async fn firmware_control_attempt_ignores_matching_info_identity_until_upgrade_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        acknowledge_publish(&mut stream).await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"info":{"command":"upgrade_confirm","sequence_id":"domain-control","result":"wrong-domain"}}"#,
        )
        .await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"domain-control","result":"correct-domain"}}"#,
        )
        .await;
        assert_eq!(read_packet(&mut stream).await.header >> 4, 14);
    });
    let mut session = connect_session(address).await;
    let mut attempt = session
        .publish(firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "domain-control".into(),
            src_id: 1,
        }))
        .await
        .unwrap();

    attempt.wait_published().await.unwrap();
    let report = attempt
        .wait_matching_report(Duration::from_secs(1))
        .await
        .unwrap();

    let acknowledgement = report
        .payload
        .firmware_acknowledgement("upgrade_confirm", "domain-control")
        .unwrap()
        .expect("matching upgrade-domain acknowledgement");
    assert_eq!(
        acknowledgement.result.as_deref(),
        Some("correct-domain"),
        "an info-domain identity must not consume a control attempt"
    );
    session.shutdown().await.unwrap();
    broker.await.unwrap();
}

#[tokio::test]
async fn firmware_refresh_attempt_ignores_matching_upgrade_identity_until_info_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        acknowledge_publish(&mut stream).await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"get_version","sequence_id":"domain-refresh","result":"wrong-domain"}}"#,
        )
        .await;
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"info":{"command":"get_version","sequence_id":"domain-refresh","module":[{"name":"future/unit","sw_ver":"9.9.9"}]}}"#,
        )
        .await;
        assert_eq!(read_packet(&mut stream).await.header >> 4, 14);
    });
    let mut session = connect_session(address).await;
    let mut attempt = session
        .publish(FirmwareMqttCommand::get_version("domain-refresh"))
        .await
        .unwrap();

    attempt.wait_published().await.unwrap();
    let report = attempt
        .wait_matching_report(Duration::from_secs(1))
        .await
        .unwrap();

    let modules = report
        .payload
        .firmware_refresh_modules()
        .unwrap()
        .expect("matching info-domain modules");
    assert_eq!(
        modules[0].name, "future/unit",
        "an upgrade-domain identity must not consume a refresh attempt"
    );
    session.shutdown().await.unwrap();
    broker.await.unwrap();
}

async fn connect_session(address: std::net::SocketAddr) -> FirmwareMqttSession {
    let mut options = MqttOptions::new(
        format!("firmware-domain-test-{}", uuid::Uuid::new_v4()),
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

async fn accept_subscription(stream: &mut tokio::net::TcpStream) {
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

async fn acknowledge_publish(stream: &mut tokio::net::TcpStream) {
    let publish = read_packet(stream).await;
    assert_eq!(publish.header >> 4, 3);
    let (_, topic_end) = mqtt_string(&publish.body, 0);
    let packet_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
    stream
        .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
        .await
        .unwrap();
}
