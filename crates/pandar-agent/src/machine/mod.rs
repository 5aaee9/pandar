pub mod file_transfer;
pub mod mqtt;

use std::time::Duration;

use async_trait::async_trait;
use mqtt::{BambuMqttTransport, refresh_printer};

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
    pub state: String,
}

#[async_trait]
pub trait BambuMachineGateway: Send + Sync {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMachineGateway;

#[async_trait]
impl BambuMachineGateway for NoopMachineGateway {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        Ok(Vec::new())
    }
}

#[derive(Debug)]
pub struct ConfiguredBambuMachineGateway<T> {
    printers: Vec<(BambuPrinterEndpoint, T)>,
    report_timeout: Duration,
}

impl<T> ConfiguredBambuMachineGateway<T> {
    pub fn new(printers: Vec<(BambuPrinterEndpoint, T)>, report_timeout: Duration) -> Self {
        Self {
            printers,
            report_timeout,
        }
    }
}

#[async_trait]
impl<T> BambuMachineGateway for ConfiguredBambuMachineGateway<T>
where
    T: BambuMqttTransport + Send + Sync,
{
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        let mut snapshots = Vec::with_capacity(self.printers.len());
        for (endpoint, transport) in &self.printers {
            snapshots.push(refresh_printer(transport, endpoint, self.report_timeout).await?);
        }
        Ok(snapshots)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::machine::mqtt::FakeMqttTransport;

    fn endpoint(serial: &str) -> BambuPrinterEndpoint {
        BambuPrinterEndpoint {
            host: "192.0.2.10".to_string(),
            serial: serial.to_string(),
            access_code: "12345678".to_string(),
            model: Some("A1 Mini".to_string()),
            name: Some(format!("printer-{serial}")),
        }
    }

    #[tokio::test]
    async fn noop_refresh_printers_returns_no_snapshots() {
        let gateway = NoopMachineGateway;

        assert_eq!(gateway.refresh_printers().await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn configured_refresh_printers_refreshes_endpoints_sequentially() {
        let first = FakeMqttTransport::with_reports([json!({"print": {"state": "READY"}})]);
        let second = FakeMqttTransport::with_reports([json!({"state": "IDLE"})]);
        let first_endpoint = endpoint("SERIAL1");
        let second_endpoint = endpoint("SERIAL2");
        let gateway = ConfiguredBambuMachineGateway::new(
            vec![
                (first_endpoint.clone(), first.clone()),
                (second_endpoint.clone(), second.clone()),
            ],
            Duration::from_secs(1),
        );

        let snapshots = gateway.refresh_printers().await.unwrap();

        assert_eq!(
            snapshots,
            vec![
                MachineSnapshot {
                    serial: "SERIAL1".to_string(),
                    name: "printer-SERIAL1".to_string(),
                    state: "READY".to_string(),
                },
                MachineSnapshot {
                    serial: "SERIAL2".to_string(),
                    name: "printer-SERIAL2".to_string(),
                    state: "IDLE".to_string(),
                },
            ]
        );
        assert_eq!(
            first.subscriptions().await,
            [format!("device/{}/report", first_endpoint.serial)]
        );
        assert_eq!(
            second.subscriptions().await,
            [format!("device/{}/report", second_endpoint.serial)]
        );
    }
}
