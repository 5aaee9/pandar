use std::{io, sync::Arc, time::Duration};

use rumqttc::{MqttOptions, QoS};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{OnceCell, oneshot},
};

use super::*;

#[test]
fn mqtt_session_client_suffix_is_unique_and_role_scoped() {
    let first = mqtt_session_client_suffix("reports");
    let second = mqtt_session_client_suffix("reports");

    assert_ne!(first, second);
    assert!(first.starts_with("reports-"));
}

#[tokio::test]
async fn publish_reaches_broker_without_calling_next_report() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_frame(&mut stream).await.unwrap();
        assert_eq!(connect.header >> 4, 1, "expected MQTT CONNECT");
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        read_publish(&mut stream).await
    });

    let options = MqttOptions::new(
        "command-pump-test",
        (address.ip().to_string(), address.port()),
    );
    let transport = RumqttcBambuMqttTransport::connect_with_options(
        options,
        "SERIAL".to_owned(),
        address.ip().to_string(),
        Arc::new(OnceCell::new_with(Some("SERIAL".to_owned()))),
        OverflowPolicy::DropOldest,
    );
    transport
        .publish(PublishedMqttCommand {
            topic: "device/SERIAL/request".to_owned(),
            payload: serde_json::json!({"print": {"command": "project_file"}}),
            qos: 1,
        })
        .await
        .unwrap();

    let publish = tokio::time::timeout(Duration::from_millis(250), broker)
        .await
        .expect("MQTT publish did not reach broker without next_report")
        .unwrap();
    assert_eq!(publish.topic, "device/SERIAL/request");
    assert_eq!(publish.qos, QoS::AtLeastOnce);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&publish.payload).unwrap(),
        serde_json::json!({"print": {"command": "project_file"}})
    );
}

#[tokio::test]
async fn queued_reports_do_not_block_publish_event_loop() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (reports_sent, reports_ready) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_frame(&mut stream).await.unwrap();
        assert_eq!(connect.header >> 4, 1, "expected MQTT CONNECT");
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        for sequence in 0..32 {
            write_report(&mut stream, sequence).await.unwrap();
        }
        reports_sent.send(()).unwrap();
        read_publish(&mut stream).await
    });

    let options = MqttOptions::new(
        "command-pump-backpressure-test",
        (address.ip().to_string(), address.port()),
    );
    let transport = RumqttcBambuMqttTransport::connect_with_options(
        options,
        "SERIAL".to_owned(),
        address.ip().to_string(),
        Arc::new(OnceCell::new_with(Some("SERIAL".to_owned()))),
        OverflowPolicy::DropOldest,
    );
    reports_ready.await.unwrap();
    tokio::time::timeout(
        Duration::from_millis(250),
        transport.pump.wait_until_report_queue_full(),
    )
    .await
    .expect("MQTT report queue did not fill");
    transport
        .publish(PublishedMqttCommand {
            topic: "device/SERIAL/request".to_owned(),
            payload: serde_json::json!({"print": {"command": "project_file"}}),
            qos: 1,
        })
        .await
        .unwrap();

    let publish = tokio::time::timeout(Duration::from_millis(250), broker)
        .await
        .expect("queued MQTT reports blocked the publish event loop")
        .unwrap();
    assert_eq!(publish.topic, "device/SERIAL/request");
}

#[tokio::test]
async fn report_precedes_connection_error_with_source_chain() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (close_connection, close_requested) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_frame(&mut stream).await.unwrap();
        assert_eq!(connect.header >> 4, 1, "expected MQTT CONNECT");
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        write_report(&mut stream, 7).await.unwrap();
        close_requested.await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let options = MqttOptions::new(
        "command-pump-order-test",
        (address.ip().to_string(), address.port()),
    );
    let transport = RumqttcBambuMqttTransport::connect_with_options(
        options,
        "SERIAL".to_owned(),
        address.ip().to_string(),
        Arc::new(OnceCell::new_with(Some("SERIAL".to_owned()))),
        OverflowPolicy::DropOldest,
    );

    let report = transport
        .next_report(Duration::from_millis(250))
        .await
        .unwrap();
    assert_eq!(report["sequence"], 7);
    close_connection.send(()).unwrap();
    let error = transport
        .next_report(Duration::from_millis(250))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("poll rumqttc event loop"));
    assert!(error.chain().count() > 1, "rumqttc source error was lost");
    broker.await.unwrap();
}

struct MqttFrame {
    header: u8,
    body: Vec<u8>,
}

struct WirePublish {
    topic: String,
    qos: QoS,
    payload: Vec<u8>,
}

async fn read_publish(stream: &mut TcpStream) -> WirePublish {
    let frame = read_frame(stream).await.unwrap();
    assert_eq!(frame.header >> 4, 3, "expected MQTT PUBLISH");
    let qos = match (frame.header >> 1) & 0x03 {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        value => panic!("invalid MQTT QoS {value}"),
    };
    let topic_len = u16::from_be_bytes([frame.body[0], frame.body[1]]) as usize;
    let topic_end = 2 + topic_len;
    let topic = String::from_utf8(frame.body[2..topic_end].to_vec()).unwrap();
    let payload_start = topic_end + usize::from(qos != QoS::AtMostOnce) * 2;
    WirePublish {
        topic,
        qos,
        payload: frame.body[payload_start..].to_vec(),
    }
}

async fn read_frame(stream: &mut TcpStream) -> io::Result<MqttFrame> {
    let header = stream.read_u8().await?;
    let mut remaining_len = 0_usize;
    let mut multiplier = 1_usize;
    loop {
        let encoded = stream.read_u8().await?;
        remaining_len += usize::from(encoded & 0x7f) * multiplier;
        if encoded & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let mut body = vec![0; remaining_len];
    stream.read_exact(&mut body).await?;
    Ok(MqttFrame { header, body })
}

async fn write_report(stream: &mut TcpStream, sequence: u8) -> io::Result<()> {
    let topic = b"device/SERIAL/report";
    let payload = format!(r#"{{"sequence":{sequence}}}"#);
    let remaining_len = 2 + topic.len() + payload.len();
    let mut frame = Vec::with_capacity(remaining_len + 2);
    frame.push(0x30);
    frame.push(u8::try_from(remaining_len).unwrap());
    frame.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    frame.extend_from_slice(topic);
    frame.extend_from_slice(payload.as_bytes());
    stream.write_all(&frame).await
}
