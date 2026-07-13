use std::sync::Arc;

use async_trait::async_trait;
use rumqttc::MqttOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
};

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, FirmwareObservationCache,
        mqtt::{FirmwareMqttSession, firmware_mqtt_options},
    },
};

use super::super::firmware_refresh::FirmwareSessionConnector;

pub(super) const REQUEST_TOPIC: &str = "device/SERIAL/request";
pub(super) const REPORT_TOPIC: &str = "device/SERIAL/report";

#[derive(Clone)]
pub(super) struct LoopbackConnector {
    address: std::net::SocketAddr,
    option_packet_sizes: Arc<Mutex<Vec<usize>>>,
}

impl LoopbackConnector {
    pub(super) fn new(address: std::net::SocketAddr) -> Self {
        Self {
            address,
            option_packet_sizes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) async fn option_packet_sizes(&self) -> Vec<usize> {
        self.option_packet_sizes.lock().await.clone()
    }
}

#[async_trait]
impl FirmwareSessionConnector for LoopbackConnector {
    async fn connect(
        &self,
        endpoint: &BambuPrinterEndpoint,
    ) -> anyhow::Result<FirmwareMqttSession> {
        let production = firmware_mqtt_options(endpoint);
        self.option_packet_sizes
            .lock()
            .await
            .push(production.max_packet_size());
        let mut options = MqttOptions::new(
            production.client_id(),
            self.address.ip().to_string(),
            self.address.port(),
        );
        options
            .set_clean_session(production.clean_session())
            .set_max_packet_size(production.max_packet_size(), production.max_packet_size());
        FirmwareMqttSession::connect_with_options(
            options,
            REQUEST_TOPIC.into(),
            REPORT_TOPIC.into(),
        )
        .await
    }
}

pub(super) async fn seeded_cache(serial: &str) -> FirmwareObservationCache {
    let cache = FirmwareObservationCache::default();
    seed_cache_entry(&cache, serial).await;
    cache
}

pub(super) async fn seed_cache_entry(cache: &FirmwareObservationCache, serial: &str) {
    let (sender, mut events) = tokio::sync::mpsc::channel(2);
    let transition = cache
        .begin_generation(&test_config(), endpoint(serial), &sender, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(transition.generation(), 1);
    drop(transition);
    events.recv().await.unwrap();
}

pub(super) fn endpoint(serial: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "127.0.0.1".into(),
        serial: serial.into(),
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

pub(super) async fn accept_subscribed_session(stream: &mut TcpStream) -> String {
    let connect = read_packet(stream).await;
    assert_eq!(connect.header >> 4, 1);
    assert_ne!(connect.body[7] & 0x02, 0);
    let client_id = mqtt_string(&connect.body, 10).0;
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    let subscribe = read_packet(stream).await;
    assert_eq!(subscribe.header >> 4, 8);
    let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
    stream
        .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x01])
        .await
        .unwrap();
    client_id
}

pub(super) async fn read_acked_command(stream: &mut TcpStream) -> serde_json::Value {
    let publish = read_packet(stream).await;
    assert_eq!(publish.header >> 4, 3);
    let (_, topic_end) = mqtt_string(&publish.body, 0);
    let packet_id = u16::from_be_bytes([publish.body[topic_end], publish.body[topic_end + 1]]);
    stream
        .write_all(&[0x40, 0x02, (packet_id >> 8) as u8, packet_id as u8])
        .await
        .unwrap();
    serde_json::from_slice(&publish.body[topic_end + 2..]).unwrap()
}

pub(super) async fn send_version_report(stream: &mut TcpStream, sequence_id: &str, version: &str) {
    let payload = serde_json::to_vec(&serde_json::json!({
        "info": {
            "command": "get_version",
            "sequence_id": sequence_id,
            "module": [
                {
                    "name": "ota",
                    "product_name": "X1 Carbon",
                    "sw_ver": version,
                    "hw_ver": "A00"
                }
            ]
        }
    }))
    .unwrap();
    write_publish(stream, REPORT_TOPIC, &payload).await;
}

pub(super) async fn expect_disconnect(stream: &mut TcpStream) {
    assert_eq!(read_packet(stream).await.header >> 4, 14);
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
