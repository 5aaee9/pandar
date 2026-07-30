use std::time::Duration;

use anyhow::{anyhow, bail};
use async_trait::async_trait;

use crate::machine::{
    BambuPrinterEndpoint, FirmwareModulesDelivery, FirmwareObservationCache,
    FirmwareRefreshRequest,
    mqtt::{FirmwareMqttCommand, FirmwareMqttSession, FirmwareMqttTaskSet},
};

const MAX_REFRESH_ATTEMPTS: usize = 3;

#[async_trait]
pub(super) trait FirmwareSessionConnector: Send + Sync {
    async fn connect(&self, endpoint: &BambuPrinterEndpoint)
    -> anyhow::Result<FirmwareMqttSession>;
}

pub(super) struct ProductionFirmwareSessionConnector {
    pub(super) task_set: FirmwareMqttTaskSet,
}

#[async_trait]
impl FirmwareSessionConnector for ProductionFirmwareSessionConnector {
    async fn connect(
        &self,
        endpoint: &BambuPrinterEndpoint,
    ) -> anyhow::Result<FirmwareMqttSession> {
        FirmwareMqttSession::connect(endpoint, self.task_set.clone()).await
    }
}

pub(super) async fn refresh_firmware_version_with_connector<C>(
    cache: &FirmwareObservationCache,
    request: FirmwareRefreshRequest,
    report_timeout: Duration,
    connector: &C,
) -> anyhow::Result<FirmwareModulesDelivery>
where
    C: FirmwareSessionConnector,
{
    let version_lease = cache.version_observation_lease(&request.serial).await;
    let mut last_error = None;
    for attempt in 1..=MAX_REFRESH_ATTEMPTS {
        let snapshot = cache
            .snapshot(&request.serial)
            .await
            .ok_or_else(|| anyhow!("no firmware endpoint for printer {}", request.serial))?;
        if snapshot.generation != request.expected_generation {
            bail!(
                "stale firmware generation {} for printer {}",
                request.expected_generation,
                request.serial
            );
        }
        match refresh_attempt(
            connector,
            &snapshot.endpoint,
            &request.sequence_id,
            report_timeout,
        )
        .await
        {
            Ok(modules) => {
                let observation = cache
                    .commit_modules(&request.serial, request.expected_generation, modules)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "printer {} firmware generation changed during refresh",
                            request.serial
                        )
                    })?;
                return Ok(FirmwareModulesDelivery::with_version_observation_lease(
                    observation,
                    version_lease,
                ));
            }
            Err(error) => {
                last_error = Some(error.context(format!(
                    "firmware refresh attempt {attempt}/{MAX_REFRESH_ATTEMPTS}"
                )));
            }
        }
    }
    Err(last_error.expect("at least one firmware refresh attempt ran"))
}

async fn refresh_attempt<C>(
    connector: &C,
    endpoint: &BambuPrinterEndpoint,
    sequence_id: &str,
    report_timeout: Duration,
) -> anyhow::Result<Vec<pandar_core::PrinterFirmwareModule>>
where
    C: FirmwareSessionConnector,
{
    let mut session = connector.connect(endpoint).await?;
    let operation = async {
        let mut attempt = session
            .publish(FirmwareMqttCommand::get_version(sequence_id))
            .await?;
        attempt.wait_published().await?;
        let report = attempt.wait_matching_report(report_timeout).await?;
        report
            .payload
            .firmware_refresh_modules()?
            .ok_or_else(|| anyhow!("matching get_version report had no firmware modules"))
    }
    .await;
    let shutdown = session.shutdown().await;
    if let Err(error) = shutdown {
        tracing::warn!(
            serial = %endpoint.serial,
            error = %format!("{error:#}"),
            "firmware refresh MQTT session shutdown was ambiguous"
        );
    }
    operation
}
