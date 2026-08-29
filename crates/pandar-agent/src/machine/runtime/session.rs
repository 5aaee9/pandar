use std::{sync::Arc, time::Duration};

use anyhow::Context;
use futures_util::{StreamExt, stream::FuturesUnordered};
use pandar_core::BambuDeviceFeatures;
use tokio::sync::mpsc;

use super::RuntimeBambuMachineGateway;
use crate::{
    AgentConfig,
    commands::authoritative_printer_snapshot_event,
    machine::{
        BambuPrinterEndpoint, ConfiguredBambuMachineGateway, DeviceFeatureCache,
        mqtt::{
            BambuMqttTransport, feature_event, observe_device_features, snapshot_from_endpoint,
        },
    },
};
use pandar_protocol::agent::v1::AgentEvent;

const MAX_CONCURRENT_SESSION_PROBES: usize = 8;

type FeatureProbeOutcome = (
    BambuPrinterEndpoint,
    u64,
    anyhow::Result<BambuDeviceFeatures>,
);

pub(super) async fn prepare_session_device_features<T, F>(
    inner: &Arc<tokio::sync::Mutex<ConfiguredBambuMachineGateway<T, F>>>,
    cache: &DeviceFeatureCache,
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    report_timeout: Duration,
) -> anyhow::Result<Vec<(String, anyhow::Error)>>
where
    T: BambuMqttTransport + Clone + Send + Sync,
{
    let targets = inner.lock().await.device_feature_probe_targets();
    for (endpoint, _, revision) in &targets {
        let gateway = inner.lock().await;
        if !gateway.device_feature_probe_is_current(endpoint, *revision) {
            continue;
        }
        let lease = cache.transition_lease(&endpoint.serial).await;
        lease.set(None);
        sender
            .send(feature_event(config, endpoint.serial.clone(), None))
            .await
            .with_context(|| {
                format!(
                    "queue printer {} device feature invalidation",
                    endpoint.serial
                )
            })?;
    }

    let outcomes = probe_device_features_bounded(targets, report_timeout).await;
    let mut failures = Vec::new();
    for (endpoint, revision, outcome) in outcomes {
        let gateway = inner.lock().await;
        if !gateway.device_feature_probe_is_current(&endpoint, revision) {
            continue;
        }
        match outcome {
            Ok(value) => {
                let lease = cache.transition_lease(&endpoint.serial).await;
                lease.set(Some(value));
                sender
                    .send(feature_event(config, endpoint.serial.clone(), Some(value)))
                    .await
                    .with_context(|| {
                        format!(
                            "queue printer {} device feature observation",
                            endpoint.serial
                        )
                    })?;
            }
            Err(error) => failures.push((endpoint.serial, error)),
        }
    }
    Ok(failures)
}

async fn probe_device_features_bounded<T>(
    targets: Vec<(BambuPrinterEndpoint, T, u64)>,
    report_timeout: Duration,
) -> Vec<FeatureProbeOutcome>
where
    T: BambuMqttTransport + Send + Sync,
{
    let target_count = targets.len();
    let mut remaining = targets.into_iter().enumerate();
    let mut probes = FuturesUnordered::new();
    for _ in 0..MAX_CONCURRENT_SESSION_PROBES {
        let Some((index, (endpoint, transport, revision))) = remaining.next() else {
            break;
        };
        probes.push(probe_device_features_target(
            index,
            endpoint,
            transport,
            revision,
            report_timeout,
        ));
    }

    let mut outcomes = (0..target_count).map(|_| None).collect::<Vec<_>>();
    while let Some((index, endpoint, revision, outcome)) = probes.next().await {
        outcomes[index] = Some((endpoint, revision, outcome));
        if let Some((next_index, (next_endpoint, next_transport, next_revision))) = remaining.next()
        {
            probes.push(probe_device_features_target(
                next_index,
                next_endpoint,
                next_transport,
                next_revision,
                report_timeout,
            ));
        }
    }
    outcomes
        .into_iter()
        .map(|outcome| outcome.expect("every session feature probe returned an outcome"))
        .collect()
}

async fn probe_device_features_target<T>(
    index: usize,
    endpoint: BambuPrinterEndpoint,
    transport: T,
    revision: u64,
    report_timeout: Duration,
) -> (
    usize,
    BambuPrinterEndpoint,
    u64,
    anyhow::Result<BambuDeviceFeatures>,
)
where
    T: BambuMqttTransport + Send + Sync,
{
    let outcome = observe_device_features(&transport, &endpoint, report_timeout).await;
    (index, endpoint, revision, outcome)
}

impl RuntimeBambuMachineGateway {
    pub(super) async fn queue_configured_printer_rows(
        &self,
        endpoints: &[BambuPrinterEndpoint],
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        for endpoint in endpoints {
            sender
                .send(authoritative_printer_snapshot_event(
                    &self.config,
                    snapshot_from_endpoint(endpoint),
                ))
                .await
                .with_context(|| {
                    format!("queue configured printer {} snapshot", endpoint.serial)
                })?;
        }
        Ok(())
    }

    pub(crate) async fn teardown_session_report_forwarders(&self) -> anyhow::Result<()> {
        let mut failure = None;
        loop {
            let serial = self.report_tasks.lock().await.keys().next().cloned();
            let Some(serial) = serial else {
                break;
            };
            if let Err(error) = self
                .stop_report_task(
                    &serial,
                    "join runtime printer report forwarder during session teardown",
                )
                .await
            {
                if failure.is_none() {
                    failure = Some(error);
                } else {
                    tracing::warn!(
                        error = %format!("{error:#}"),
                        "additional printer report forwarder teardown failure"
                    );
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(super) async fn stop_report_task(
        &self,
        serial: &str,
        context: &'static str,
    ) -> anyhow::Result<bool> {
        let mut tasks = self.report_tasks.lock().await;
        let Some(task) = tasks.get_mut(serial) else {
            return Ok(false);
        };
        #[cfg(test)]
        self.pause_report_join_for_test_if_installed(serial).await;
        task.abort();
        let result = (&mut *task).await;
        tasks.remove(serial);
        match result {
            Ok(()) => Ok(true),
            Err(error) if error.is_cancelled() => Ok(true),
            Err(error) => Err(anyhow::Error::new(error).context(context)),
        }
    }
}
