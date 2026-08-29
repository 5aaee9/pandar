use std::time::Duration;

use anyhow::anyhow;

use crate::machine::mqtt::transport::mqtt_report_idle_timeout;

use super::{
    AttemptEvent, FirmwareMqttAttempt, FirmwareMqttReport,
    pump::{FirmwareMqttOperationPhase, attempt_failure, attempt_pump_failure},
};

impl FirmwareMqttAttempt {
    pub(crate) async fn wait_published(&mut self) -> anyhow::Result<()> {
        if self.published {
            return Ok(());
        }
        match self.events.recv().await {
            Some(AttemptEvent::Published) => {
                self.published = true;
                Ok(())
            }
            Some(AttemptEvent::Failed {
                after_publish,
                error,
            }) => Err(attempt_pump_failure(after_publish, error)),
            Some(AttemptEvent::Report(_)) => Err(attempt_failure(
                false,
                FirmwareMqttOperationPhase::Receive,
                anyhow!("firmware MQTT report preceded own outgoing publish"),
            )),
            None => Err(attempt_failure(
                false,
                FirmwareMqttOperationPhase::Send,
                anyhow!("firmware MQTT pump ended before own publish"),
            )),
        }
    }

    pub(crate) async fn wait_matching_report(
        &mut self,
        report_timeout: Duration,
    ) -> anyhow::Result<FirmwareMqttReport> {
        if !self.published {
            self.wait_published().await?;
        }
        match tokio::time::timeout(report_timeout, self.events.recv()).await {
            Ok(Some(AttemptEvent::Report(report))) => Ok(*report),
            Ok(Some(AttemptEvent::Failed {
                after_publish,
                error,
            })) => Err(attempt_pump_failure(after_publish, error)),
            Ok(Some(AttemptEvent::Published)) => Err(attempt_failure(
                true,
                FirmwareMqttOperationPhase::Send,
                anyhow!("firmware MQTT emitted duplicate outgoing publish"),
            )),
            Ok(None) => Err(attempt_failure(
                true,
                FirmwareMqttOperationPhase::Receive,
                anyhow!("firmware MQTT pump ended after publish"),
            )),
            Err(_) => Err(mqtt_report_idle_timeout(report_timeout)),
        }
    }
}
