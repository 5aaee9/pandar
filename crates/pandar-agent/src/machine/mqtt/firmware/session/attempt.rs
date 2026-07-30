use std::time::Duration;

use crate::machine::mqtt::transport::mqtt_report_idle_timeout;

use super::{AttemptEvent, FirmwareMqttAttempt, FirmwareMqttReport, attempt_failure};

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
            }) => Err(attempt_failure(after_publish, error)),
            Some(AttemptEvent::Report(_)) => Err(attempt_failure(
                false,
                "firmware MQTT report preceded own outgoing publish".into(),
            )),
            None => Err(attempt_failure(
                false,
                "firmware MQTT pump ended before own publish".into(),
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
            })) => Err(attempt_failure(after_publish, error)),
            Ok(Some(AttemptEvent::Published)) => Err(attempt_failure(
                true,
                "firmware MQTT emitted duplicate outgoing publish".into(),
            )),
            Ok(None) => Err(attempt_failure(
                true,
                "firmware MQTT pump ended after publish".into(),
            )),
            Err(_) => Err(mqtt_report_idle_timeout(report_timeout)),
        }
    }
}
