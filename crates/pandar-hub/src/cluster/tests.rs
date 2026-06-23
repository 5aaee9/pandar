use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use pandar_core::{AgentId, TenantId};
use tokio::sync::Mutex;

use super::*;
use crate::db::DatabaseBackend;

#[derive(Debug, Clone)]
struct PublishedMessage {
    subject: String,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
struct RecordingNatsTransport {
    published: Mutex<Vec<PublishedMessage>>,
    payloads: Mutex<Vec<Vec<u8>>>,
}

#[async_trait]
impl NatsTransport for RecordingNatsTransport {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()> {
        self.published
            .lock()
            .await
            .push(PublishedMessage { subject, payload });
        Ok(())
    }

    async fn subscribe(&self, _subject: String) -> anyhow::Result<NatsPayloadStream> {
        let payloads = self
            .payloads
            .lock()
            .await
            .drain(..)
            .map(Ok)
            .collect::<Vec<_>>();
        Ok(Box::pin(futures_util::stream::iter(payloads)))
    }
}

#[test]
fn control_plane_config_defaults_to_in_process() {
    let config = ControlPlaneConfig::from_values(DatabaseBackend::Postgres, None, None, None)
        .expect("config should parse");

    assert_eq!(config, ControlPlaneConfig::InProcess);
}

#[test]
fn control_plane_config_rejects_sqlite_nats() {
    let err = ControlPlaneConfig::from_values(
        DatabaseBackend::Sqlite,
        Some("nats"),
        Some("nats://127.0.0.1:4222"),
        None,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("SQLite deployments do not support"));
}

#[test]
fn control_plane_config_requires_nats_url() {
    let err = ControlPlaneConfig::from_values(DatabaseBackend::Postgres, Some("nats"), None, None)
        .unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_NATS_URL"));
}

#[test]
fn control_plane_config_defaults_nats_subject() {
    let config = ControlPlaneConfig::from_values(
        DatabaseBackend::Postgres,
        Some("nats"),
        Some("nats://127.0.0.1:4222"),
        None,
    )
    .expect("config should parse");

    assert_eq!(
        config,
        ControlPlaneConfig::Nats {
            url: "nats://127.0.0.1:4222".to_string(),
            subject: DEFAULT_NATS_SUBJECT.to_string(),
        }
    );
}

#[tokio::test]
async fn nats_control_plane_publishes_subject_and_json_payload() {
    let transport = Arc::new(RecordingNatsTransport::default());
    let control_plane = NatsControlPlane::new(transport.clone(), "pandar.test".to_string());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();

    control_plane
        .publish(HubControlMessage::AgentWake {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
        })
        .await
        .unwrap();

    let published = transport.published.lock().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].subject, "pandar.test");
    let decoded: HubControlMessage = serde_json::from_slice(&published[0].payload).unwrap();
    assert!(matches!(
        decoded,
        HubControlMessage::AgentWake {
            tenant_id: decoded_tenant_id,
            agent_id: decoded_agent_id,
        } if decoded_tenant_id == tenant_id.to_string()
            && decoded_agent_id == agent_id.to_string()
    ));
}

#[tokio::test]
async fn nats_control_plane_subscribe_decodes_json_payloads() {
    let transport = Arc::new(RecordingNatsTransport::default());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    transport.payloads.lock().await.push(
        serde_json::to_vec(&HubControlMessage::AgentClose {
            tenant_id: tenant_id.to_string(),
            agent_id: agent_id.to_string(),
        })
        .unwrap(),
    );
    let control_plane = NatsControlPlane::new(transport, "pandar.test".to_string());

    let mut stream = control_plane.subscribe().await.unwrap();
    let message = stream.next().await.unwrap().unwrap();

    assert!(matches!(
        message,
        HubControlMessage::AgentClose {
            tenant_id: decoded_tenant_id,
            agent_id: decoded_agent_id,
        } if decoded_tenant_id == tenant_id.to_string()
            && decoded_agent_id == agent_id.to_string()
    ));
}

#[tokio::test]
async fn nats_control_plane_subscribe_reports_decode_errors_and_continues() {
    let transport = Arc::new(RecordingNatsTransport::default());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    {
        let mut payloads = transport.payloads.lock().await;
        payloads.push(b"{".to_vec());
        payloads.push(
            serde_json::to_vec(&HubControlMessage::AgentWake {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
            })
            .unwrap(),
        );
    }
    let control_plane = NatsControlPlane::new(transport, "pandar.test".to_string());

    let mut stream = control_plane.subscribe().await.unwrap();
    let err = stream.next().await.unwrap().unwrap_err();
    let message = stream.next().await.unwrap().unwrap();

    assert!(format!("{err:#}").contains("failed to decode control message"));
    assert!(matches!(
        message,
        HubControlMessage::AgentWake {
            tenant_id: decoded_tenant_id,
            agent_id: decoded_agent_id,
        } if decoded_tenant_id == tenant_id.to_string()
            && decoded_agent_id == agent_id.to_string()
    ));
}

#[test]
fn parse_agent_identity_reads_tenant_and_agent_ids() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();

    let parsed = parse_agent_identity(&tenant_id.to_string(), &agent_id.to_string()).unwrap();

    assert_eq!(parsed, (tenant_id, agent_id));
}
