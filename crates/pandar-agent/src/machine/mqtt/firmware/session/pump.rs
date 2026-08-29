use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::anyhow;
use rumqttc::{AsyncClient, Event, EventLoop, Outgoing, Packet, QoS, SubscribeReasonCode};
use tokio::sync::{mpsc, oneshot};

use crate::machine::{
    FirmwarePublishTransition,
    mqtt::{
        BAMBU_MQTT_RETAIN, MachineReport, decode_mqtt_report_payload,
        firmware::FirmwareResponseDomain,
    },
};

use super::{FirmwareBarrierPause, FirmwareMqttCommand, FirmwareMqttReport};

pub(super) enum PumpRequest {
    Publish {
        command: FirmwareMqttCommand,
        events: mpsc::UnboundedSender<AttemptEvent>,
        transition: Option<Box<FirmwarePublishTransition>>,
    },
    Shutdown {
        done: oneshot::Sender<Result<(), FirmwareMqttPumpFailure>>,
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
    Report(Box<FirmwareMqttReport>),
    Failed {
        after_publish: bool,
        error: FirmwareMqttPumpFailure,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FirmwareMqttOperationPhase {
    Subscribe,
    Send,
    Receive,
    Shutdown,
    Session,
}

impl fmt::Display for FirmwareMqttOperationPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Subscribe => "subscribe",
            Self::Send => "send",
            Self::Receive => "receive",
            Self::Shutdown => "shutdown",
            Self::Session => "session",
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct FirmwareMqttPumpFailure {
    phase: FirmwareMqttOperationPhase,
    source: Arc<anyhow::Error>,
}

impl FirmwareMqttPumpFailure {
    fn new(phase: FirmwareMqttOperationPhase, source: anyhow::Error) -> Self {
        Self {
            phase,
            source: Arc::new(source),
        }
    }
}

impl fmt::Display for FirmwareMqttPumpFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "firmware MQTT {} operation failed", self.phase)
    }
}

impl Error for FirmwareMqttPumpFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref().as_ref())
    }
}

#[derive(Debug)]
pub(super) struct FirmwareMqttAttemptFailure {
    pub(super) after_publish: bool,
    source: FirmwareMqttPumpFailure,
}

impl fmt::Display for FirmwareMqttAttemptFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.after_publish {
            formatter.write_str("firmware MQTT failed after publish; outcome unknown")
        } else {
            formatter.write_str("firmware MQTT failed before publish")
        }
    }
}

impl Error for FirmwareMqttAttemptFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

pub(super) async fn run_pump(
    client: AsyncClient,
    mut event_loop: EventLoop,
    request_topic: String,
    mut requests: mpsc::Receiver<PumpRequest>,
    suback: oneshot::Sender<Result<(), FirmwareMqttPumpFailure>>,
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
                        send_attempt_failure(
                            &events,
                            false,
                            FirmwareMqttOperationPhase::Session,
                            anyhow!("fresh firmware MQTT session already has an attempt"),
                        );
                        continue;
                    }
                    let barrier = received_ordinal.load(Ordering::SeqCst);
                    if let Some(pause) = barrier_pause.take() {
                        let _ = pause.reached.send(());
                        if pause.release.await.is_err() {
                            send_attempt_failure(
                                &events,
                                false,
                                FirmwareMqttOperationPhase::Send,
                                anyhow!("firmware MQTT barrier pause was cancelled"),
                            );
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
                        send_attempt_failure(
                            &events,
                            false,
                            FirmwareMqttOperationPhase::Send,
                            anyhow::Error::new(error).context("queue firmware MQTT publish"),
                        );
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
                            let failure = pump_failure(
                                FirmwareMqttOperationPhase::Shutdown,
                                anyhow!("firmware shutdown completion error sentinel"),
                            );
                            let _ = done.send(Err(failure));
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
                            let failure = pump_failure(
                                FirmwareMqttOperationPhase::Shutdown,
                                anyhow::Error::new(error)
                                    .context("queue firmware MQTT disconnect"),
                            );
                            let _ = done.send(Err(failure.clone()));
                            return Err(anyhow::Error::new(failure));
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
                        Err(pump_failure(
                            FirmwareMqttOperationPhase::Subscribe,
                            anyhow!("unexpected firmware MQTT SUBACK packet id {}", ack.pkid),
                        ))
                    } else if ack.return_codes.contains(&SubscribeReasonCode::Failure) {
                        Err(pump_failure(
                            FirmwareMqttOperationPhase::Subscribe,
                            anyhow!("firmware MQTT subscription was rejected"),
                        ))
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
                            .map(MachineReport::decode)
                            .and_then(|report| {
                                report
                                    .firmware_report_matches(
                                        active.response_domain,
                                        &active.command,
                                        &active.sequence_id,
                                    )
                                    .map(|matches| (matches, report))
                            });
                        match report {
                            Ok((true, report)) => {
                                let _ = active.events.send(AttemptEvent::Report(Box::new(
                                    FirmwareMqttReport {
                                        #[cfg(test)]
                                        ordinal,
                                        payload: report,
                                    },
                                )));
                            }
                            Ok((false, _)) => {}
                            Err(error) => {
                                let failure = pump_failure(
                                    FirmwareMqttOperationPhase::Receive,
                                    error.context("process firmware MQTT report"),
                                );
                                let _ = active.events.send(AttemptEvent::Failed {
                                    after_publish: true,
                                    error: failure.clone(),
                                });
                                if let Some(done) = shutdown.take() {
                                    let _ = done.send(Err(failure.clone()));
                                }
                                return Err(anyhow::Error::new(failure));
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
                    let phase = if suback.is_some() {
                        FirmwareMqttOperationPhase::Subscribe
                    } else if shutdown.is_some() {
                        FirmwareMqttOperationPhase::Shutdown
                    } else {
                        FirmwareMqttOperationPhase::Receive
                    };
                    let failure = pump_failure(
                        phase,
                        anyhow::Error::new(error).context("poll firmware MQTT event loop"),
                    );
                    if let Some(suback) = suback.take() {
                        let _ = suback.send(Err(failure.clone()));
                    }
                    if let Some(active) = &active {
                        let _ = active.events.send(AttemptEvent::Failed {
                            after_publish: active.published,
                            error: failure.clone(),
                        });
                    }
                    if let Some(done) = shutdown.take() {
                        let _ = done.send(Err(failure.clone()));
                    }
                    return Err(anyhow::Error::new(failure));
                }
            }
        }
    }
}

fn send_attempt_failure(
    events: &mpsc::UnboundedSender<AttemptEvent>,
    after_publish: bool,
    phase: FirmwareMqttOperationPhase,
    source: anyhow::Error,
) {
    let _ = events.send(AttemptEvent::Failed {
        after_publish,
        error: pump_failure(phase, source),
    });
}

pub(super) fn pump_failure(
    phase: FirmwareMqttOperationPhase,
    source: anyhow::Error,
) -> FirmwareMqttPumpFailure {
    FirmwareMqttPumpFailure::new(phase, source)
}

pub(super) fn attempt_failure(
    after_publish: bool,
    phase: FirmwareMqttOperationPhase,
    source: anyhow::Error,
) -> anyhow::Error {
    attempt_pump_failure(after_publish, pump_failure(phase, source))
}

pub(super) fn attempt_pump_failure(
    after_publish: bool,
    source: FirmwareMqttPumpFailure,
) -> anyhow::Error {
    anyhow::Error::new(FirmwareMqttAttemptFailure {
        after_publish,
        source,
    })
}
