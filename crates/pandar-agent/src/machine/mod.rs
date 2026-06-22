pub mod file_transfer;
pub mod mqtt;

use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use file_transfer::{MachineFileTransfer, TransferModeCache, run_with_transfer_mode};
use mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, ProjectFileCommand,
    PublishedMqttCommand, refresh_printer,
};

use crate::protocol::agent::v1::PrintProjectFile;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BambuPrinterEndpoint {
    pub host: String,
    pub serial: String,
    pub access_code: String,
    pub model: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSnapshot {
    pub serial: String,
    pub name: String,
    pub model: Option<String>,
    pub state: String,
}

#[async_trait]
pub trait BambuMachineGateway: Send + Sync {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>>;
    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()>;
    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMachineGateway;

#[async_trait]
impl BambuMachineGateway for NoopMachineGateway {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        Ok(Vec::new())
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        _command: &PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }
}

#[derive(Debug)]
pub struct ConfiguredBambuMachineGateway<T, F = UnavailableMachineFileTransfer> {
    printers: Vec<(BambuPrinterEndpoint, T, F)>,
    report_timeout: Duration,
    transfer_cache: TransferModeCache,
}

impl<T> ConfiguredBambuMachineGateway<T> {
    pub fn new(printers: Vec<(BambuPrinterEndpoint, T)>, report_timeout: Duration) -> Self {
        Self {
            printers: printers
                .into_iter()
                .map(|(endpoint, mqtt)| (endpoint, mqtt, UnavailableMachineFileTransfer))
                .collect(),
            report_timeout,
            transfer_cache: TransferModeCache::default(),
        }
    }
}

#[async_trait]
impl<T, F> BambuMachineGateway for ConfiguredBambuMachineGateway<T, F>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Send + Sync,
{
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        let mut snapshots = Vec::with_capacity(self.printers.len());
        for (endpoint, transport, _) in &self.printers {
            snapshots.push(refresh_printer(transport, endpoint, self.report_timeout).await?);
        }
        Ok(snapshots)
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        if self
            .printers
            .iter()
            .any(|(endpoint, _, _)| endpoint.serial == serial_number)
        {
            return Ok(());
        }

        bail!("no configured Bambu printer matches serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        let Some((endpoint, mqtt, transfer)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };

        dispatch_print_project_file(
            endpoint,
            transfer,
            mqtt,
            &self.transfer_cache,
            command,
            &artifact,
        )
        .await
    }
}

impl<T, F> ConfiguredBambuMachineGateway<T, F> {
    pub fn with_file_transfer(
        printers: Vec<(BambuPrinterEndpoint, T, F)>,
        report_timeout: Duration,
        transfer_cache: TransferModeCache,
    ) -> Self {
        Self {
            printers,
            report_timeout,
            transfer_cache,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableMachineFileTransfer;

#[async_trait]
impl MachineFileTransfer for UnavailableMachineFileTransfer {
    async fn list(
        &self,
        _path: &str,
        _mode: file_transfer::TransferProtectionMode,
    ) -> anyhow::Result<Vec<String>> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn download(
        &self,
        _path: &str,
        _mode: file_transfer::TransferProtectionMode,
    ) -> anyhow::Result<Vec<u8>> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn upload(
        &self,
        _path: &str,
        _bytes: &[u8],
        _mode: file_transfer::TransferProtectionMode,
    ) -> anyhow::Result<()> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }

    async fn delete(
        &self,
        _path: &str,
        _mode: file_transfer::TransferProtectionMode,
    ) -> anyhow::Result<()> {
        bail!("Bambu FTPS runtime is not implemented in this phase")
    }
}

async fn dispatch_print_project_file<F, T>(
    endpoint: &BambuPrinterEndpoint,
    transfer: &F,
    mqtt: &T,
    cache: &TransferModeCache,
    command: &PrintProjectFile,
    artifact: &[u8],
) -> anyhow::Result<()>
where
    F: MachineFileTransfer + Send + Sync,
    T: BambuMqttTransport + Send + Sync,
{
    let remote_path = command.filename.clone();
    run_with_transfer_mode(endpoint, cache, false, |mode| {
        let remote_path = remote_path.clone();
        async move { transfer.upload(&remote_path, artifact, mode).await }
    })
    .await
    .with_context(|| format!("upload print artifact to {}", endpoint.serial))?;

    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request,
        payload: BambuMqttCommand::ProjectFile(ProjectFileCommand {
            filename: command.filename.clone(),
            plate_id: command.plate_id,
            task_id: command.job_id.clone(),
            subtask_id: command.artifact_id.clone(),
            use_ams: command.use_ams,
            flow_cali: command.flow_cali,
            timelapse: command.timelapse,
        })
        .payload(),
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .with_context(|| format!("publish project_file to {}", endpoint.serial))
}

#[cfg(test)]
mod tests;
