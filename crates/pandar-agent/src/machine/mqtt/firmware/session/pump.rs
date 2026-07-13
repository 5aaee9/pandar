use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, anyhow};
use rumqttc::{AsyncClient, Event, EventLoop, Outgoing, Packet, QoS, SubscribeReasonCode};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::machine::{
    FirmwarePublishTransition,
    mqtt::{BAMBU_MQTT_RETAIN, decode_mqtt_report_payload, firmware::FirmwareResponseDomain},
};

use super::{FirmwareBarrierPause, FirmwareMqttCommand, FirmwareMqttReport};

pub(super) enum PumpRequest {
    Publish {
        command: FirmwareMqttCommand,
        events: mpsc::UnboundedSender<AttemptEvent>,
        transition: Option<Box<FirmwarePublishTransition>>,
    },
    Shutdown {
        done: oneshot::Sender<Result<(), String>>,
        #[cfg(test)]
        completion_mode: ShutdownCompletionMode,
    },
    #[cfg(test)]
    Panic(oneshot::Sender<()>),
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum ShutdownCompletionMode {
    Normal,
    Error,
    Drop,
}

pub(super) enum AttemptEvent {
    Published,
    Report(FirmwareMqttReport),
    Failed { after_publish: bool, error: String },
}

struct ActiveCommand {
    response_domain: FirmwareResponseDomain,
    command: String,
    sequence_id: String,
    barrier: u64,
    published: bool,
    events: mpsc::UnboundedSender<AttemptEvent>,
    transition: Option<Box<FirmwarePublishTransition>>,
}

#[derive(Debug)]
pub(super) struct FirmwareMqttAttemptFailure {
    pub(super) after_publish: bool,
    message: String,
}

impl fmt::Display for FirmwareMqttAttemptFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.after_publish {
            write!(
                formatter,
                "firmware MQTT failed after publish; outcome unknown: {}",
                self.message
            )
        } else {
            write!(
                formatter,
                "firmware MQTT failed before publish: {}",
                self.message
            )
        }
    }
}

impl std::error::Error for FirmwareMqttAttemptFailure {}

pub(super) async fn run_pump(
    client: AsyncClient,
    mut event_loop: EventLoop,
    request_topic: String,
    mut requests: mpsc::Receiver<PumpRequest>,
    suback: oneshot::Sender<Result<(), String>>,
    received_ordinal: Arc<AtomicU64>,
    mut barrier_pause: Option<FirmwareBarrierPause>,
) -> anyhow::Result<()> {
    let mut suback = Some(suback);
    let mut subscribe_packet_id = None;
    let mut active: Option<ActiveCommand> = None;
    let mut shutdown = None;
    loop {
        tokio::select! {
            biased;
            request = requests.recv() => match request {
                Some(PumpRequest::Publish {
                    command,
                    events,
                    transition,
                }) => {
                    if active.is_some() {
                        let _ = events.send(AttemptEvent::Failed {
                            after_publish: false,
                            error: "fresh firmware MQTT session already has an attempt".into(),
                        });
                        continue;
                    }
                    let barrier = received_ordinal.load(Ordering::SeqCst);
                    if let Some(pause) = barrier_pause.take() {
                        let _ = pause.reached.send(());
                        if pause.release.await.is_err() {
                            let _ = events.send(AttemptEvent::Failed {
                                after_publish: false,
                                error: "firmware MQTT barrier pause was cancelled".into(),
                            });
                            continue;
                        }
                    }
                    let publish = client.publish(
                        &request_topic,
                        QoS::AtLeastOnce,
                        BAMBU_MQTT_RETAIN,
                        command.payload_bytes().to_vec(),
                    ).await;
                    if let Err(error) = publish {
                        let _ = events.send(AttemptEvent::Failed {
                            after_publish: false,
                            error: format!("queue firmware MQTT publish: {error:#}"),
                        });
                        continue;
                    }
                    active = Some(ActiveCommand {
                        response_domain: command.response_domain(),
                        command: command.command().to_owned(),
                        sequence_id: command.sequence_id().to_owned(),
                        barrier,
                        published: false,
                        events,
                        transition,
                    });
                }
                Some(PumpRequest::Shutdown {
                    done,
                    #[cfg(test)]
                    completion_mode,
                }) => {
                    #[cfg(test)]
                    match completion_mode {
                        ShutdownCompletionMode::Normal => {}
                        ShutdownCompletionMode::Error => {
                            let _ = done.send(Err(
                                "firmware shutdown completion error sentinel".into(),
                            ));
                            return Err(anyhow!("firmware shutdown pump error sentinel"));
                        }
                        ShutdownCompletionMode::Drop => {
                            drop(done);
                            return Err(anyhow!("firmware shutdown sender-drop pump sentinel"));
                        }
                    }
                    match client.disconnect().await {
                        Ok(()) => shutdown = Some(done),
                        Err(error) => {
                            let message = format!("queue firmware MQTT disconnect: {error:#}");
                            let _ = done.send(Err(message.clone()));
                            return Err(anyhow!(message));
                        }
                    }
                }
                #[cfg(test)]
                Some(PumpRequest::Panic(reached)) => {
                    let _ = reached.send(());
                    panic!("firmware parent-owned pump panic sentinel");
                }
                None => return Ok(()),
            },
            polled = event_loop.poll() => match polled {
                Ok(Event::Outgoing(Outgoing::Subscribe(packet_id))) => {
                    subscribe_packet_id = Some(packet_id);
                }
                Ok(Event::Incoming(Packet::SubAck(ack))) => {
                    let result = if Some(ack.pkid) != subscribe_packet_id {
                        Err(format!("unexpected firmware MQTT SUBACK packet id {}", ack.pkid))
                    } else if ack.return_codes.contains(&SubscribeReasonCode::Failure) {
                        Err("firmware MQTT subscription was rejected".into())
                    } else {
                        Ok(())
                    };
                    if let Some(suback) = suback.take() {
                        let _ = suback.send(result);
                    }
                }
                Ok(Event::Outgoing(Outgoing::Publish(_))) => {
                    if let Some(active) = &mut active
                        && !active.published
                    {
                        active.published = true;
                        drop(active.transition.take());
                        let _ = active.events.send(AttemptEvent::Published);
                    }
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    let ordinal = received_ordinal.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(active) = &active
                        && active.published
                        && ordinal > active.barrier
                    {
                        let report = decode_mqtt_report_payload(publish.payload.as_ref())
                            .and_then(|payload| {
                                report_matches(
                                    &payload,
                                    active.response_domain,
                                    &active.command,
                                    &active.sequence_id,
                                )
                                    .map(|matches| (matches, payload))
                            });
                        match report {
                            Ok((true, payload)) => {
                                let _ = active.events.send(AttemptEvent::Report(FirmwareMqttReport {
                                    #[cfg(test)]
                                    ordinal,
                                    payload,
                                }));
                            }
                            Ok((false, _)) => {}
                            Err(error) => {
                                let message = format!("process firmware MQTT report: {error:#}");
                                let _ = active.events.send(AttemptEvent::Failed {
                                    after_publish: true,
                                    error: message.clone(),
                                });
                                if let Some(done) = shutdown.take() {
                                    let _ = done.send(Err(message));
                                }
                                return Err(error).context("process firmware MQTT report");
                            }
                        }
                    }
                }
                Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    if let Some(done) = shutdown.take() {
                        let _ = done.send(Ok(()));
                    }
                    return Ok(());
                }
                Ok(_) => {}
                Err(error) => {
                    let message = format!("poll firmware MQTT event loop: {error:#}");
                    if let Some(suback) = suback.take() {
                        let _ = suback.send(Err(message.clone()));
                    }
                    if let Some(active) = &active {
                        let _ = active.events.send(AttemptEvent::Failed {
                            after_publish: active.published,
                            error: message.clone(),
                        });
                    }
                    if let Some(done) = shutdown.take() {
                        let _ = done.send(Err(message.clone()));
                    }
                    return Err(anyhow!(message));
                }
            }
        }
    }
}

pub(super) fn attempt_failure(after_publish: bool, error: String) -> anyhow::Error {
    anyhow::Error::new(FirmwareMqttAttemptFailure {
        after_publish,
        message: error,
    })
}

fn report_matches(
    report: &Value,
    response_domain: FirmwareResponseDomain,
    command: &str,
    sequence_id: &str,
) -> anyhow::Result<bool> {
    let envelope = serde_json::from_value::<ReportIdentityEnvelope>(report.clone())
        .context("parse firmware MQTT response identity")?;
    let identity = match response_domain {
        FirmwareResponseDomain::Info => envelope.info,
        FirmwareResponseDomain::Upgrade => envelope.upgrade,
    };
    Ok(identity.is_some_and(|identity| {
        identity.command.as_deref() == Some(command)
            && identity.sequence_id.as_deref() == Some(sequence_id)
    }))
}

#[derive(Deserialize)]
struct ReportIdentityEnvelope {
    #[serde(default)]
    info: Option<ReportIdentity>,
    #[serde(default)]
    upgrade: Option<ReportIdentity>,
}

#[derive(Deserialize)]
struct ReportIdentity {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    sequence_id: Option<String>,
}
