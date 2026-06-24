use std::{pin::Pin, sync::Arc};

use anyhow::{Context, bail};
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use pandar_core::{AgentId, TenantId};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

use crate::{db::DatabaseBackend, printer_events::PrinterEvent};

pub const DEFAULT_NATS_SUBJECT: &str = "pandar.hub.control";

pub type ControlMessageStream =
    Pin<Box<dyn Stream<Item = anyhow::Result<HubControlMessage>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlPlaneConfig {
    InProcess,
    Nats { url: String, subject: String },
}

impl ControlPlaneConfig {
    pub fn from_values(
        database_backend: DatabaseBackend,
        control_plane: Option<&str>,
        nats_url: Option<&str>,
        nats_subject: Option<&str>,
    ) -> anyhow::Result<Self> {
        let control_plane = control_plane
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("in-process");

        match control_plane {
            "in-process" => Ok(Self::InProcess),
            "nats" => {
                if database_backend == DatabaseBackend::Sqlite {
                    bail!("SQLite deployments do not support the NATS control plane");
                }
                let url = nats_url
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .context("PANDAR_NATS_URL is required when PANDAR_CONTROL_PLANE=nats")?
                    .to_string();
                let subject = nats_subject
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(DEFAULT_NATS_SUBJECT)
                    .to_string();
                Ok(Self::Nats { url, subject })
            }
            value => bail!("unsupported PANDAR_CONTROL_PLANE value {value}"),
        }
    }

    pub fn from_env(database_backend: DatabaseBackend) -> anyhow::Result<Self> {
        let control_plane = std::env::var("PANDAR_CONTROL_PLANE").ok();
        let nats_url = std::env::var("PANDAR_NATS_URL").ok();
        let nats_subject = std::env::var("PANDAR_NATS_SUBJECT").ok();
        Self::from_values(
            database_backend,
            control_plane.as_deref(),
            nats_url.as_deref(),
            nats_subject.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HubControlMessage {
    AgentWake {
        tenant_id: String,
        agent_id: String,
    },
    AgentClose {
        tenant_id: String,
        agent_id: String,
    },
    PrinterEvent {
        tenant_id: String,
        event: PrinterEvent,
    },
}

pub(crate) fn parse_tenant_id(value: &str) -> anyhow::Result<TenantId> {
    TenantId::parse(value).with_context(|| format!("failed to parse tenant id {value}"))
}

pub(crate) fn parse_agent_identity(
    tenant_id: &str,
    agent_id: &str,
) -> anyhow::Result<(TenantId, AgentId)> {
    let tenant_id = parse_tenant_id(tenant_id)?;
    let agent_id =
        AgentId::parse(agent_id).with_context(|| format!("failed to parse agent id {agent_id}"))?;
    Ok((tenant_id, agent_id))
}

#[derive(Clone)]
pub struct ControlPlane {
    backend: Arc<dyn ControlPlaneBackend>,
    kind: ControlPlaneKind,
}

impl std::fmt::Debug for ControlPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlPlane")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlPlaneKind {
    InProcess,
    Nats,
}

impl ControlPlane {
    pub fn in_process() -> Self {
        Self {
            backend: Arc::new(InProcessControlPlane::new()),
            kind: ControlPlaneKind::InProcess,
        }
    }

    pub async fn from_config(config: ControlPlaneConfig) -> anyhow::Result<Self> {
        match config {
            ControlPlaneConfig::InProcess => Ok(Self::in_process()),
            ControlPlaneConfig::Nats { url, subject } => {
                let client = async_nats::connect(&url)
                    .await
                    .with_context(|| format!("failed to connect to NATS at {url}"))?;
                Ok(Self {
                    backend: Arc::new(NatsControlPlane::new(
                        Arc::new(AsyncNatsTransport { client }),
                        subject,
                    )),
                    kind: ControlPlaneKind::Nats,
                })
            }
        }
    }

    pub fn kind(&self) -> ControlPlaneKind {
        self.kind
    }

    pub async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        self.backend.publish(message).await
    }

    pub async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        self.backend.subscribe().await
    }

    #[cfg(test)]
    pub(crate) fn failing_for_tests() -> Self {
        Self {
            backend: Arc::new(FailingControlPlaneBackend),
            kind: ControlPlaneKind::InProcess,
        }
    }

    #[cfg(test)]
    pub(crate) fn nats_for_tests() -> Self {
        Self {
            backend: Arc::new(InProcessControlPlane::new()),
            kind: ControlPlaneKind::Nats,
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
struct FailingControlPlaneBackend;

#[cfg(test)]
#[async_trait]
impl ControlPlaneBackend for FailingControlPlaneBackend {
    async fn publish(&self, _message: HubControlMessage) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("test control plane publish failure"))
    }

    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        Ok(Box::pin(futures_util::stream::empty()))
    }
}

#[async_trait]
pub trait ControlPlaneBackend: Send + Sync {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()>;
    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream>;
}

#[derive(Debug)]
pub struct InProcessControlPlane {
    sender: broadcast::Sender<HubControlMessage>,
}

impl InProcessControlPlane {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }
}

impl Default for InProcessControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ControlPlaneBackend for InProcessControlPlane {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        let _ = self.sender.send(message);
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        let stream = BroadcastStream::new(self.sender.subscribe()).filter_map(|item| async move {
            match item {
                Ok(message) => Some(Ok(message)),
                Err(BroadcastStreamRecvError::Lagged(skipped)) => Some(Err(anyhow::anyhow!(
                    "control plane subscriber lagged by {skipped} messages"
                ))),
            }
        });
        Ok(Box::pin(stream))
    }
}

#[async_trait]
pub trait NatsTransport: Send + Sync {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()>;
    async fn subscribe(&self, subject: String) -> anyhow::Result<NatsPayloadStream>;
}

pub type NatsPayloadStream = Pin<Box<dyn Stream<Item = anyhow::Result<Vec<u8>>> + Send + 'static>>;

#[derive(Debug)]
pub struct AsyncNatsTransport {
    client: async_nats::Client,
}

#[async_trait]
impl NatsTransport for AsyncNatsTransport {
    async fn publish(&self, subject: String, payload: Vec<u8>) -> anyhow::Result<()> {
        self.client
            .publish(subject, payload.into())
            .await
            .context("failed to publish NATS control message")
    }

    async fn subscribe(&self, subject: String) -> anyhow::Result<NatsPayloadStream> {
        let subscriber = self
            .client
            .subscribe(subject)
            .await
            .context("failed to subscribe to NATS control subject")?;
        let stream = subscriber.map(|message| Ok(message.payload.to_vec()));
        Ok(Box::pin(stream))
    }
}

pub struct NatsControlPlane {
    transport: Arc<dyn NatsTransport>,
    subject: String,
}

impl NatsControlPlane {
    pub fn new(transport: Arc<dyn NatsTransport>, subject: String) -> Self {
        Self { transport, subject }
    }
}

#[async_trait]
impl ControlPlaneBackend for NatsControlPlane {
    async fn publish(&self, message: HubControlMessage) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(&message).context("failed to encode control message")?;
        self.transport.publish(self.subject.clone(), payload).await
    }

    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        let stream = self
            .transport
            .subscribe(self.subject.clone())
            .await?
            .map(|item| {
                let payload = item?;
                serde_json::from_slice(&payload).context("failed to decode control message")
            });
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests;
