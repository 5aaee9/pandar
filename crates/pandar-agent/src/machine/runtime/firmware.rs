use std::{sync::Arc, time::Duration};

#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{
        Condvar, Mutex as StdMutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Context;
use futures_util::{StreamExt, stream::FuturesUnordered};
use tokio::{sync::mpsc, time::sleep};

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, ConfiguredBambuMachineGateway, DeviceFeatureCache,
        FirmwareObservationCache, FirmwareReportContext, PrinterRefreshResult,
        diagnostics::redact_access_code,
        mqtt::{
            BambuMqttTransport, MqttForwardingContext, MqttPresenceState, feature_event,
            forward_print_reports_with_context, refresh_printer_with_firmware,
        },
    },
    protocol::agent::v1::AgentEvent,
};

#[cfg(test)]
static REFRESH_CHILD_DROP_PAUSES: OnceLock<
    StdMutex<HashMap<String, Arc<RefreshChildDropPauseState>>>,
> = OnceLock::new();

#[cfg(test)]
struct RefreshChildDropPauseState {
    started: AtomicBool,
    dropped: AtomicBool,
    started_notify: tokio::sync::Notify,
    released: StdMutex<bool>,
    released_notify: Condvar,
}

#[cfg(test)]
pub(crate) struct RefreshChildDropPause {
    state: Arc<RefreshChildDropPauseState>,
}

#[cfg(test)]
pub(crate) fn pause_refresh_child_drop_for_test(serial: &str) -> RefreshChildDropPause {
    let state = Arc::new(RefreshChildDropPauseState {
        started: AtomicBool::new(false),
        dropped: AtomicBool::new(false),
        started_notify: tokio::sync::Notify::new(),
        released: StdMutex::new(false),
        released_notify: Condvar::new(),
    });
    REFRESH_CHILD_DROP_PAUSES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(serial.to_owned(), Arc::clone(&state));
    RefreshChildDropPause { state }
}

#[cfg(test)]
impl RefreshChildDropPause {
    pub(crate) async fn wait_until_started(&self) {
        while !self.state.started.load(Ordering::SeqCst) {
            self.state.started_notify.notified().await;
        }
    }

    pub(crate) fn release(&self) {
        *self.state.released.lock().unwrap() = true;
        self.state.released_notify.notify_all();
    }

    pub(crate) fn was_dropped(&self) -> bool {
        self.state.dropped.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Drop for RefreshChildDropPause {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
struct RefreshChildDropGuard(Option<Arc<RefreshChildDropPauseState>>);

#[cfg(test)]
impl RefreshChildDropGuard {
    fn new(serial: &str) -> Self {
        let state = REFRESH_CHILD_DROP_PAUSES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .remove(serial);
        Self(state)
    }
}

#[cfg(test)]
impl Drop for RefreshChildDropGuard {
    fn drop(&mut self) {
        let Some(state) = &self.0 else {
            return;
        };
        state.started.store(true, Ordering::SeqCst);
        state.started_notify.notify_one();
        let released = state.released.lock().unwrap();
        drop(
            state
                .released_notify
                .wait_while(released, |released| !*released)
                .unwrap(),
        );
        state.dropped.store(true, Ordering::SeqCst);
    }
}

pub(crate) struct RuntimeReportContext {
    pub(crate) device_features: DeviceFeatureCache,
    pub(crate) firmware: FirmwareReportContext,
}

pub(crate) async fn refresh_runtime_printers_with_firmware<T, F>(
    inner: Arc<tokio::sync::Mutex<ConfiguredBambuMachineGateway<T, F>>>,
    firmware: FirmwareObservationCache,
    device_features: DeviceFeatureCache,
    event_context: Option<(AgentConfig, mpsc::Sender<AgentEvent>)>,
    report_timeout: Duration,
) -> anyhow::Result<Vec<PrinterRefreshResult>>
where
    T: BambuMqttTransport + Clone + Send + Sync + 'static,
    F: Send + 'static,
{
    let serials = inner
        .lock()
        .await
        .endpoints()
        .into_iter()
        .map(|endpoint| endpoint.serial)
        .collect::<Vec<_>>();
    let mut refreshes = FuturesUnordered::new();
    for (index, serial) in serials.iter().cloned().enumerate() {
        let inner = Arc::clone(&inner);
        let firmware = firmware.clone();
        let device_features = device_features.clone();
        let event_context = event_context.clone();
        refreshes.push(async move {
            #[cfg(test)]
            let _drop_guard = RefreshChildDropGuard::new(&serial);
            let _lease = firmware.version_observation_lease(&serial).await;
            let (endpoint, transport) = inner
                .lock()
                .await
                .refresh_target(&serial)
                .with_context(|| format!("no configured Bambu printer matches serial {serial}"))?;
            let generation = firmware
                .snapshot(&serial)
                .await
                .filter(|snapshot| snapshot.endpoint == endpoint)
                .map(|snapshot| snapshot.generation);
            let (result, observation) =
                refresh_printer_with_firmware(&transport, &endpoint, report_timeout).await?;
            if let Some(value) = result.snapshot.device_features {
                device_features.update(&serial, value).await;
            }
            if let (Some(generation), Some((config, sender))) = (generation, event_context) {
                firmware
                    .commit_report_modules(
                        &config,
                        &serial,
                        generation,
                        observation.modules,
                        &sender,
                    )
                    .await?;
            }
            Ok::<_, anyhow::Error>((index, result))
        });
    }

    let mut results = (0..serials.len()).map(|_| None).collect::<Vec<_>>();
    while let Some(result) = refreshes.next().await {
        let (index, result) = result?;
        results[index] = Some(result);
    }
    Ok(results
        .into_iter()
        .map(|result| result.expect("every runtime refresh task returned a result"))
        .collect())
}

pub(crate) async fn forward_print_reports_with_firmware_retry<T>(
    config: AgentConfig,
    transport: T,
    endpoint: BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: mpsc::Sender<AgentEvent>,
    retry_delay: Duration,
    mut context: RuntimeReportContext,
) where
    T: BambuMqttTransport + Send + Sync,
{
    let mut presence = MqttPresenceState::default();
    loop {
        match forward_print_reports_with_context(
            &config,
            &transport,
            &endpoint,
            report_timeout,
            &sender,
            MqttForwardingContext {
                device_features: &context.device_features,
                firmware: Some(context.firmware.clone()),
                presence: &mut presence,
            },
        )
        .await
        {
            Ok(()) => return,
            Err(err) => {
                let error = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %error,
                    "printer report forwarding failed; retrying"
                );
                let transition = match context
                    .firmware
                    .cache
                    .begin_generation(
                        &config,
                        endpoint.clone(),
                        &sender,
                        Some(context.firmware.generation),
                    )
                    .await
                {
                    Ok(Some(transition)) => transition,
                    Ok(None) => return,
                    Err(error) => {
                        tracing::warn!(
                            serial = %endpoint.serial,
                            error = %format!("{error:#}"),
                            "printer firmware generation invalidation failed"
                        );
                        return;
                    }
                };
                context.firmware.generation = transition.generation();
                drop(transition);
                context.device_features.invalidate(&endpoint.serial).await;
                if sender
                    .send(feature_event(&config, endpoint.serial.clone(), None))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        tokio::select! {
            _ = sender.closed() => return,
            _ = sleep(retry_delay) => {}
        }
    }
}
