#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::{
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use anyhow::{Context, anyhow};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::machine::{
    BambuPrinterEndpoint,
    mqtt::transport::{bambu_lan_mqtt_options, resolved_request_topic},
};

#[cfg(test)]
use super::pump::ShutdownCompletionMode;
use super::pump::run_pump;
use super::{FirmwareBarrierPause, FirmwareMqttSession, FirmwareMqttTaskSet};
#[cfg(test)]
use super::{FirmwarePumpDropPause, drop_pause::PumpDropPauseFuture};
#[cfg(test)]
use std::sync::atomic::Ordering;

const SESSION_QUEUE_CAPACITY: usize = 10;
const SUBACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct ConnectHooks {
    barrier_pause: Option<FirmwareBarrierPause>,
    #[cfg(test)]
    pump_drop_pause: Option<FirmwarePumpDropPause>,
    #[cfg(test)]
    pump_finished: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    pump_reaped: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    registration_waiting: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    cleanup_pause: Option<FirmwareBarrierPause>,
}

impl FirmwareMqttSession {
    pub(crate) async fn connect(
        endpoint: &BambuPrinterEndpoint,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        let request_topic = resolved_request_topic(endpoint).await?;
        let report_topic = format!(
            "{}/report",
            request_topic
                .strip_suffix("/request")
                .expect("resolved Bambu request topic ends in /request")
        );
        Self::connect_inner(
            firmware_mqtt_options(endpoint),
            request_topic,
            report_topic,
            ConnectHooks::default(),
            task_set,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks::default(),
            FirmwareMqttTaskSet::default(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_task_set(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks::default(),
            task_set,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_barrier_pause(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        barrier_pause: FirmwareBarrierPause,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                barrier_pause: Some(barrier_pause),
                ..ConnectHooks::default()
            },
            FirmwareMqttTaskSet::default(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_barrier_pause_and_task_set(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        barrier_pause: FirmwareBarrierPause,
        pump_drop_pause: FirmwarePumpDropPause,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                barrier_pause: Some(barrier_pause),
                pump_drop_pause: Some(pump_drop_pause),
                ..ConnectHooks::default()
            },
            task_set,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_pump_finished(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        pump_finished: Arc<AtomicBool>,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                pump_finished: Some(pump_finished),
                ..ConnectHooks::default()
            },
            FirmwareMqttTaskSet::default(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_pump_finished_and_task_set(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        pump_finished: Arc<AtomicBool>,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                pump_finished: Some(pump_finished),
                ..ConnectHooks::default()
            },
            task_set,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_pump_guards_and_task_set(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        pump_finished: Arc<AtomicBool>,
        pump_reaped: Arc<AtomicBool>,
        registration_waiting: Arc<AtomicBool>,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                pump_finished: Some(pump_finished),
                pump_reaped: Some(pump_reaped),
                registration_waiting: Some(registration_waiting),
                ..ConnectHooks::default()
            },
            task_set,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_with_options_and_cleanup_pause(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        pump_finished: Arc<AtomicBool>,
        cleanup_pause: FirmwareBarrierPause,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        Self::connect_inner(
            options,
            request_topic,
            report_topic,
            ConnectHooks {
                pump_finished: Some(pump_finished),
                cleanup_pause: Some(cleanup_pause),
                ..ConnectHooks::default()
            },
            task_set,
        )
        .await
    }

    async fn connect_inner(
        options: MqttOptions,
        request_topic: String,
        report_topic: String,
        mut hooks: ConnectHooks,
        task_set: FirmwareMqttTaskSet,
    ) -> anyhow::Result<Self> {
        let (client, event_loop) = AsyncClient::builder(options)
            .capacity(SESSION_QUEUE_CAPACITY)
            .build();
        client
            .subscribe(&report_topic, QoS::AtLeastOnce)
            .await
            .with_context(|| format!("queue firmware MQTT subscription to {report_topic}"))?;
        let (requests, request_receiver) = mpsc::channel(1);
        let (suback_sender, suback_receiver) = oneshot::channel();
        let received_ordinal = Arc::new(AtomicU64::new(0));
        let pump_ordinal = Arc::clone(&received_ordinal);
        #[cfg(test)]
        let pump_finished = hooks
            .pump_finished
            .take()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        #[cfg(test)]
        let pump_finished_marker = Arc::clone(&pump_finished);
        #[cfg(test)]
        let pump_owner_finished = Some(Arc::clone(&pump_finished));
        #[cfg(test)]
        let pump_reaped = hooks
            .pump_reaped
            .take()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        #[cfg(test)]
        let pump_owner_reaped = Some(Arc::clone(&pump_reaped));
        #[cfg(not(test))]
        let pump_owner_finished = None;
        #[cfg(not(test))]
        let pump_owner_reaped = None;
        let barrier_pause = hooks.barrier_pause.take();
        let pump_future = async move {
            let result = run_pump(
                client,
                event_loop,
                request_topic,
                request_receiver,
                suback_sender,
                pump_ordinal,
                barrier_pause,
            )
            .await;
            #[cfg(test)]
            pump_finished_marker.store(true, Ordering::SeqCst);
            result
        };
        #[cfg(test)]
        let pump_future = PumpDropPauseFuture::new(pump_future, hooks.pump_drop_pause.take());
        #[cfg(test)]
        if let Some(registration_waiting) = hooks.registration_waiting.take() {
            registration_waiting.store(true, Ordering::SeqCst);
        }
        let mut pump = task_set
            .spawn(pump_future, pump_owner_finished, pump_owner_reaped)
            .await;
        let abort = pump.abort_handle();
        match tokio::time::timeout(SUBACK_TIMEOUT, suback_receiver).await {
            Ok(Ok(Ok(()))) => Ok(Self {
                requests,
                pump: Some(pump),
                abort,
                #[cfg(test)]
                received_ordinal,
                #[cfg(test)]
                pump_finished,
                #[cfg(test)]
                pump_reaped,
                #[cfg(test)]
                shutdown_pause: None,
                #[cfg(test)]
                shutdown_completion_mode: ShutdownCompletionMode::Normal,
            }),
            Ok(Ok(Err(error))) => {
                #[cfg(test)]
                pause_connect_cleanup(&mut hooks.cleanup_pause).await;
                let failure =
                    anyhow::Error::new(error).context("subscribe fresh firmware MQTT session");
                Err(connect_failure_with_cleanup(
                    failure,
                    pump.abort_and_join().await,
                ))
            }
            Ok(Err(_)) => {
                let result = pump
                    .join()
                    .await
                    .context("join failed firmware MQTT pump")?;
                result.context("start fresh firmware MQTT pump")?;
                Err(anyhow!("firmware MQTT pump ended before SUBACK"))
            }
            Err(_) => {
                #[cfg(test)]
                pause_connect_cleanup(&mut hooks.cleanup_pause).await;
                let failure = anyhow!("timed out waiting for firmware MQTT SUBACK");
                Err(connect_failure_with_cleanup(
                    failure,
                    pump.abort_and_join().await,
                ))
            }
        }
    }
}

pub(super) fn connect_failure_with_cleanup(
    failure: anyhow::Error,
    cleanup: Result<anyhow::Result<()>, tokio::task::JoinError>,
) -> anyhow::Error {
    match cleanup {
        Ok(Ok(())) => failure,
        Err(error) if error.is_cancelled() => failure,
        Ok(Err(error)) => {
            failure.context(format!("firmware MQTT pump cleanup also failed: {error:#}"))
        }
        Err(error) => failure.context(format!("firmware MQTT pump join also failed: {error:#}")),
    }
}

#[cfg(test)]
async fn pause_connect_cleanup(pause: &mut Option<FirmwareBarrierPause>) {
    if let Some(pause) = pause.take() {
        let _ = pause.reached.send(());
        let _ = pause.release.await;
    }
}

pub(crate) fn firmware_mqtt_options(endpoint: &BambuPrinterEndpoint) -> MqttOptions {
    let serial = endpoint
        .serial
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(22)
        .collect::<String>();
    let serial = if serial.is_empty() {
        "printer"
    } else {
        &serial
    };
    let client_id = format!("pandar-agent-fw-{serial}-{}", Uuid::new_v4());
    let mut options = bambu_lan_mqtt_options(endpoint, None);
    options.set_client_id(client_id).set_clean_session(true);
    options
}
