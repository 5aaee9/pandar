use anyhow::Context;
use tokio::sync::mpsc;

use super::RuntimeBambuMachineGateway;
use crate::{
    commands::authoritative_printer_snapshot_event,
    machine::{BambuPrinterEndpoint, mqtt::snapshot_from_endpoint},
    protocol::agent::v1::AgentEvent,
};

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
