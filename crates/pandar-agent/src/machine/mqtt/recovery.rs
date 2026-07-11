use std::{future::Future, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Outgoing, Packet, QoS};

use crate::machine::{BambuPrinterEndpoint, PrinterOperationDispatchResult};

use super::{
    BAMBU_MQTT_RETAIN, BambuMqttCommand, bambu_lan_mqtt_options, transport::resolved_request_topic,
};

const RECOVERY_DEADLINE: Duration = Duration::from_secs(5);

pub(in crate::machine) async fn dispatch_sequence_zero_recovery(
    endpoint: &BambuPrinterEndpoint,
    command: BambuMqttCommand,
) -> anyhow::Result<PrinterOperationDispatchResult> {
    dispatch_with_deadline(dispatch_attempt(endpoint, command), RECOVERY_DEADLINE).await
}

async fn dispatch_attempt(
    endpoint: &BambuPrinterEndpoint,
    command: BambuMqttCommand,
) -> anyhow::Result<PrinterOperationDispatchResult> {
    let options = recovery_mqtt_options(endpoint);
    let topic = resolved_request_topic(endpoint).await?;

    dispatch_rumqttc_attempt(options, topic, command).await
}

pub(super) async fn dispatch_rumqttc_attempt(
    options: MqttOptions,
    topic: String,
    command: BambuMqttCommand,
) -> anyhow::Result<PrinterOperationDispatchResult> {
    let (client, event_loop) = AsyncClient::new(options, 1);
    dispatch_with_attempt(
        RumqttcRecoveryAttempt { client, event_loop },
        topic,
        command,
    )
    .await
}

pub(super) async fn dispatch_with_deadline<F>(
    dispatch: F,
    deadline: Duration,
) -> anyhow::Result<PrinterOperationDispatchResult>
where
    F: Future<Output = anyhow::Result<PrinterOperationDispatchResult>>,
{
    tokio::time::timeout(deadline, dispatch)
        .await
        .context("timed out dispatching sequence-zero recovery through MQTT PUBACK")?
}

pub(super) fn recovery_mqtt_options(endpoint: &BambuPrinterEndpoint) -> MqttOptions {
    let suffix = format!("recovery-{}", uuid::Uuid::new_v4());
    let mut options = bambu_lan_mqtt_options(endpoint, Some(&suffix));
    options.set_clean_session(true);
    options
}

pub(super) async fn dispatch_with_attempt<A>(
    mut attempt: A,
    topic: String,
    command: BambuMqttCommand,
) -> anyhow::Result<PrinterOperationDispatchResult>
where
    A: RecoveryAttempt,
{
    let payload = serde_json::to_vec(&command.payload())
        .context("encode sequence-zero recovery MQTT payload")?;
    attempt
        .publish(topic, QoS::AtLeastOnce, BAMBU_MQTT_RETAIN, payload)
        .await
        .context("enqueue sequence-zero recovery MQTT publish")?;

    let mut own_packet_id = None;
    loop {
        match attempt
            .poll()
            .await
            .context("poll recovery MQTT event loop")?
        {
            Event::Outgoing(Outgoing::Publish(packet_id)) => own_packet_id = Some(packet_id),
            Event::Incoming(Packet::PubAck(ack)) if own_packet_id == Some(ack.pkid) => {
                return Ok(PrinterOperationDispatchResult {
                    sequence_id: Some("0".to_owned()),
                    error: None,
                    mqtt_report: None,
                    mqtt_summary: None,
                });
            }
            Event::Incoming(_) | Event::Outgoing(_) => {}
        }
    }
}

#[async_trait]
pub(super) trait RecoveryAttempt: Send {
    async fn publish(
        &mut self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> anyhow::Result<()>;

    async fn poll(&mut self) -> anyhow::Result<Event>;
}

struct RumqttcRecoveryAttempt {
    client: AsyncClient,
    event_loop: EventLoop,
}

#[async_trait]
impl RecoveryAttempt for RumqttcRecoveryAttempt {
    async fn publish(
        &mut self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.client
            .publish(topic, qos, retain, payload)
            .await
            .map_err(Into::into)
    }

    async fn poll(&mut self) -> anyhow::Result<Event> {
        self.event_loop.poll().await.map_err(Into::into)
    }
}
