use std::{io, net::SocketAddr, time::Duration};

use rumqttc::MqttOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub(super) const TEST_REQUEST_TOPIC: &str = "device/01S00EXAMPLE/request";

pub(super) struct WirePublish {
    pub(super) topic: String,
    pub(super) packet_id: u16,
    pub(super) qos: u8,
    pub(super) retain: bool,
    pub(super) payload: Vec<u8>,
}

struct MqttFrame {
    header: u8,
    body: Vec<u8>,
}

pub(super) fn local_mqtt_options(address: SocketAddr, client_id: &str) -> MqttOptions {
    let mut options = MqttOptions::new(client_id, (address.ip().to_string(), address.port()));
    options.set_clean_session(true);
    options.set_keep_alive(30);
    options
}

pub(super) async fn accept_connection(
    listener: &TcpListener,
    expected_client_id: &str,
) -> TcpStream {
    let (mut stream, _) = listener.accept().await.unwrap();
    let connect = read_frame(&mut stream).await.unwrap();
    assert_eq!(connect.header >> 4, 1, "expected MQTT CONNECT");
    assert_eq!(connect_client_id(&connect), expected_client_id);
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    stream
}

pub(super) async fn read_publish(stream: &mut TcpStream) -> WirePublish {
    let frame = read_frame(stream).await.unwrap();
    assert_eq!(frame.header >> 4, 3, "expected MQTT PUBLISH");
    let qos = (frame.header >> 1) & 0x03;
    assert_eq!(qos, 1, "expected QoS1 PUBLISH");
    let topic_len = u16::from_be_bytes([frame.body[0], frame.body[1]]) as usize;
    let topic_end = 2 + topic_len;
    let topic = String::from_utf8(frame.body[2..topic_end].to_vec()).unwrap();
    let packet_id = u16::from_be_bytes([frame.body[topic_end], frame.body[topic_end + 1]]);
    WirePublish {
        topic,
        packet_id,
        qos,
        retain: frame.header & 0x01 != 0,
        payload: frame.body[topic_end + 2..].to_vec(),
    }
}

pub(super) async fn assert_no_packet_before_ack(stream: &mut TcpStream) {
    match tokio::time::timeout(Duration::from_millis(50), read_frame(stream)).await {
        Err(_) => {}
        Ok(Ok(frame)) => panic!(
            "unexpected MQTT packet type {} before recovery PUBACK",
            frame.header >> 4
        ),
        Ok(Err(err)) => panic!("recovery connection ended before PUBACK: {err}"),
    }
}

pub(super) async fn send_puback(stream: &mut TcpStream, packet_id: u16) -> io::Result<()> {
    let [high, low] = packet_id.to_be_bytes();
    stream.write_all(&[0x40, 0x02, high, low]).await
}

fn connect_client_id(frame: &MqttFrame) -> String {
    let protocol_len = u16::from_be_bytes([frame.body[0], frame.body[1]]) as usize;
    let client_len_offset = 2 + protocol_len + 4;
    let client_len = u16::from_be_bytes([
        frame.body[client_len_offset],
        frame.body[client_len_offset + 1],
    ]) as usize;
    let client_start = client_len_offset + 2;
    String::from_utf8(frame.body[client_start..client_start + client_len].to_vec()).unwrap()
}

async fn read_frame(stream: &mut TcpStream) -> io::Result<MqttFrame> {
    let header = stream.read_u8().await?;
    let mut remaining_len = 0usize;
    let mut multiplier = 1usize;
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
