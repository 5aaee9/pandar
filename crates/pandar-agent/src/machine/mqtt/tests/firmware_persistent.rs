use std::{collections::HashSet, sync::Arc, time::Duration};

use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Barrier, mpsc, watch},
    time::timeout,
};

use super::super::firmware::{FirmwareMqttCommand, FirmwareMqttSession, firmware_mqtt_options};
use crate::machine::BambuPrinterEndpoint;

const REQUEST_TOPIC: &str = "device/SERIAL/request";
const REPORT_TOPIC: &str = "device/SERIAL/report";

#[tokio::test]
async fn fresh_firmware_sessions_do_not_disconnect_persistent_command_or_report_clients() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let persistent_connected = Arc::new(Barrier::new(3));
    let (probe_seen, mut probes) = mpsc::unbounded_channel();
    let (finish, finish_receiver) = watch::channel(false);
    let broker = tokio::spawn({
        let persistent_connected = Arc::clone(&persistent_connected);
        async move {
            let mut tasks = Vec::new();
            for _ in 0..4 {
                let (stream, _) = listener.accept().await.unwrap();
                let persistent_connected = Arc::clone(&persistent_connected);
                let probe_seen = probe_seen.clone();
                let finish = finish_receiver.clone();
                tasks.push(tokio::spawn(handle_client(
                    stream,
                    persistent_connected,
                    probe_seen,
                    finish,
                )));
            }
            let mut firmware_ids = Vec::new();
            for task in tasks {
                if let Some(client_id) = task.await.unwrap() {
                    firmware_ids.push(client_id);
                }
            }
            firmware_ids
        }
    });

    let (command_client, command_loop) = persistent_client("pandar-agent-command", address);
    let (report_client, report_loop) = persistent_client("pandar-agent-report", address);
    let command_pump = tokio::spawn(poll_persistent(command_loop));
    let report_pump = tokio::spawn(poll_persistent(report_loop));
    persistent_connected.wait().await;

    for sequence in ["fresh-one", "fresh-two"] {
        let mut session = connect_firmware_session(address).await;
        let mut attempt = session
            .publish(FirmwareMqttCommand::get_version(sequence))
            .await
            .unwrap();
        attempt.wait_published().await.unwrap();
        session.shutdown().await.unwrap();
    }

    command_client
        .publish("persistent/probe", QoS::AtMostOnce, false, "command")
        .await
        .unwrap();
    report_client
        .publish("persistent/probe", QoS::AtMostOnce, false, "report")
        .await
        .unwrap();
    let observed = timeout(Duration::from_secs(1), async {
        let mut observed = HashSet::new();
        while observed.len() < 2 {
            observed.insert(probes.recv().await.unwrap());
        }
        observed
    })
    .await
    .unwrap();
    assert_eq!(
        observed,
        HashSet::from([
            "pandar-agent-command".to_owned(),
            "pandar-agent-report".to_owned()
        ])
    );
    finish.send(true).unwrap();
    let firmware_ids = broker.await.unwrap();
    assert_eq!(firmware_ids.len(), 2);
    assert_ne!(firmware_ids[0], firmware_ids[1]);
    assert!(
        firmware_ids
            .iter()
            .all(|id| id.starts_with("pandar-agent-fw-SERIAL-"))
    );

    drop(command_client);
    drop(report_client);
    command_pump.abort();
    report_pump.abort();
    let _ = command_pump.await;
    let _ = report_pump.await;
}

fn persistent_client(client_id: &str, address: std::net::SocketAddr) -> (AsyncClient, EventLoop) {
    let mut options = MqttOptions::new(client_id, address.ip().to_string(), address.port());
    options.set_keep_alive(Duration::from_secs(30));
    AsyncClient::new(options, 10)
}

async fn poll_persistent(mut event_loop: EventLoop) {
    while event_loop.poll().await.is_ok() {}
}

async fn connect_firmware_session(address: std::net::SocketAddr) -> FirmwareMqttSession {
    let endpoint = BambuPrinterEndpoint {
        host: "127.0.0.1".into(),
        serial: "SERIAL".into(),
        access_code: "secret".into(),
        model: None,
        name: None,
    };
    let production = firmware_mqtt_options(&endpoint);
    let mut options = MqttOptions::new(
        production.client_id(),
        address.ip().to_string(),
        address.port(),
    );
    options
        .set_clean_session(production.clean_session())
        .set_max_packet_size(production.max_packet_size(), production.max_packet_size());
    FirmwareMqttSession::connect_with_options(options, REQUEST_TOPIC.into(), REPORT_TOPIC.into())
        .await
        .unwrap()
}

async fn handle_client(
    mut stream: TcpStream,
    persistent_connected: Arc<Barrier>,
    probe_seen: mpsc::UnboundedSender<String>,
    mut finish: watch::Receiver<bool>,
) -> Option<String> {
    let connect = read_packet(&mut stream).await;
    let client_id = mqtt_string(&connect.body, 10).0;
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    if matches!(
        client_id.as_str(),
        "pandar-agent-command" | "pandar-agent-report"
    ) {
        persistent_connected.wait().await;
        loop {
            tokio::select! {
                packet = read_packet(&mut stream) => {
                    assert_ne!(packet.header >> 4, 14, "firmware shutdown disconnected {client_id}");
                    if packet.header >> 4 == 3 {
                        probe_seen.send(client_id.clone()).unwrap();
                    }
                }
                changed = finish.changed() => {
                    changed.unwrap();
                    assert!(*finish.borrow());
                    return None;
                }
            }
        }
    }
    let subscribe = read_packet(&mut stream).await;
    assert_eq!(subscribe.header >> 4, 8);
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
    assert_eq!(publish.header >> 4, 3);
    let (_, topic_end) = mqtt_string(&publish.body, 0);
    let publish_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
    stream
        .write_all(&[0x40, 0x02, (publish_id >> 8) as u8, publish_id as u8])
        .await
        .unwrap();
    match stream.read_u8().await {
        Ok(header) => assert_eq!(header >> 4, 14),
        Err(error) => assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
            ),
            "unexpected firmware client close: {error}"
        ),
    }
    Some(client_id)
}

struct Packet {
    header: u8,
    body: Vec<u8>,
}

async fn read_packet(stream: &mut TcpStream) -> Packet {
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

fn mqtt_string(body: &[u8], offset: usize) -> (String, usize) {
    let length = usize::from(u16::from_be_bytes([body[offset], body[offset + 1]]));
    let start = offset + 2;
    let end = start + length;
    (String::from_utf8(body[start..end].to_vec()).unwrap(), end)
}
