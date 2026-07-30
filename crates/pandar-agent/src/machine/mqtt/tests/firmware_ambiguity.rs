use std::time::Duration;

use pandar_core::FirmwareCommand;
use rumqttc::MqttOptions;
use tokio::{io::AsyncWriteExt, net::TcpListener};

use super::super::firmware::{FirmwareMqttSession, firmware_command_payload};
use super::firmware_session::{
    REPORT_TOPIC, REQUEST_TOPIC, mqtt_string, read_packet, write_publish,
};

#[tokio::test]
async fn delayed_old_ack_with_reused_command_and_sequence_remains_wire_indistinguishable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        let subscribe = read_packet(&mut stream).await;
        let subscribe_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        stream
            .write_all(&[
                0x90,
                0x03,
                (subscribe_id >> 8) as u8,
                subscribe_id as u8,
                0x01,
            ])
            .await
            .unwrap();
        let publish = read_packet(&mut stream).await;
        let (_, topic_end) = mqtt_string(&publish.body, 0);
        let publish_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
        stream
            .write_all(&[0x40, 0x02, (publish_id >> 8) as u8, publish_id as u8])
            .await
            .unwrap();
        write_publish(
            &mut stream,
            REPORT_TOPIC,
            br#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"reused","result":"success","reason":"delayed-old-wire-indistinguishable"}}"#,
        )
        .await;
        assert_eq!(read_packet(&mut stream).await.header >> 4, 14);
    });
    let mut options = MqttOptions::new(
        "pandar-agent-fw-ambiguity",
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
        .publish(firmware_command_payload(&FirmwareCommand::UpgradeConfirm {
            sequence_id: "reused".into(),
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
        .firmware_acknowledgement("upgrade_confirm", "reused")
        .unwrap()
        .unwrap();

    assert_eq!(
        acknowledgement.reason.as_deref(),
        Some("delayed-old-wire-indistinguishable")
    );
    session.shutdown().await.unwrap();
    broker.await.unwrap();
}
