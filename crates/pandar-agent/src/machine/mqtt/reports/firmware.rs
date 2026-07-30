use std::time::Duration;

use anyhow::Context;
use tokio::sync::{OwnedMutexGuard, mpsc};

use crate::{
    AgentConfig,
    machine::{BambuPrinterEndpoint, FirmwareReportContext, FirmwareReportReducer},
    protocol::agent::v1::AgentEvent,
};

use super::super::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, MachineReport,
    MachineReports, PublishedMqttCommand,
};

pub(super) struct FirmwareReportProcessor {
    serial: String,
    context: FirmwareReportContext,
    reducer: FirmwareReportReducer,
    version_lease: Option<OwnedMutexGuard<()>>,
    version_deadline: tokio::time::Instant,
}

impl FirmwareReportProcessor {
    pub(super) async fn start<T>(
        endpoint: &BambuPrinterEndpoint,
        context: FirmwareReportContext,
        report_timeout: Duration,
        reports: &MachineReports<'_, T>,
        topics: &BambuMqttTopics,
    ) -> anyhow::Result<Self>
    where
        T: BambuMqttTransport + ?Sized,
    {
        let version_lease = context
            .cache
            .version_observation_lease(&endpoint.serial)
            .await;
        reports
            .publish(PublishedMqttCommand {
                topic: topics.request.clone(),
                payload: BambuMqttCommand::GetVersion.payload(),
                qos: BAMBU_MQTT_QOS,
            })
            .await
            .with_context(|| format!("publish get_version to request topic {}", topics.request))?;
        Ok(Self {
            reducer: FirmwareReportReducer::new(&endpoint.serial, context.generation),
            serial: endpoint.serial.clone(),
            context,
            version_lease: Some(version_lease),
            version_deadline: tokio::time::Instant::now() + report_timeout,
        })
    }

    pub(super) async fn observe(
        &mut self,
        config: &AgentConfig,
        report: &MachineReport,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        match report.firmware_refresh_modules() {
            Ok(Some(modules)) => {
                let version_lease = match self.version_lease.take() {
                    Some(lease) => lease,
                    None => {
                        self.context
                            .cache
                            .version_observation_lease(&self.serial)
                            .await
                    }
                };
                self.context
                    .cache
                    .commit_report_modules(
                        config,
                        &self.serial,
                        self.context.generation,
                        modules,
                        sender,
                    )
                    .await?;
                drop(version_lease);
            }
            Ok(None) => {}
            Err(error) => {
                self.version_lease.take();
                tracing::warn!(
                    serial = %self.serial,
                    error = %format!("{error:#}"),
                    "invalid printer firmware version observation"
                );
            }
        }
        self.expire_version_observation();
        match self
            .reducer
            .observe_and_commit(report, &self.context.cache, config, sender)
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => {}
            Err(error) => tracing::warn!(
                serial = %self.serial,
                error = %format!("{error:#}"),
                "invalid printer firmware status observation"
            ),
        }
        Ok(())
    }

    pub(super) fn expire_version_observation(&mut self) {
        if tokio::time::Instant::now() >= self.version_deadline {
            self.version_lease.take();
        }
    }
}
