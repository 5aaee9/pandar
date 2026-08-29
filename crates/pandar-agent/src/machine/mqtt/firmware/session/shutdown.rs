use anyhow::{Context, anyhow};
use tokio::sync::oneshot;

#[cfg(test)]
use super::{FirmwareBarrierPauseHandle, ShutdownCompletionMode, firmware_barrier_pause};
use super::{FirmwareMqttSession, PumpRequest};

impl FirmwareMqttSession {
    pub(crate) async fn shutdown(&mut self) -> anyhow::Result<()> {
        #[cfg(test)]
        if let Some(pause) = self.shutdown_pause.take() {
            let _ = pause.reached.send(());
            let _ = pause.release.await;
        }
        let Some(pump) = self.pump.as_mut() else {
            return Ok(());
        };
        let (done, completed) = oneshot::channel();
        let completion_result = if self
            .requests
            .send(PumpRequest::Shutdown {
                done,
                #[cfg(test)]
                completion_mode: self.shutdown_completion_mode,
            })
            .await
            .is_err()
        {
            Err(anyhow!("firmware MQTT pump ended before shutdown request"))
        } else {
            match completed.await {
                Ok(result) => result.map_err(anyhow::Error::new),
                Err(error) => Err(anyhow::Error::new(error)
                    .context("firmware MQTT pump dropped shutdown completion")),
            }
        };
        let pump_result = match pump.join().await {
            Ok(result) => result.context("run firmware MQTT pump"),
            Err(error) => Err(anyhow::Error::new(error).context("join firmware MQTT pump")),
        };
        self.pump = None;
        match (completion_result, pump_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Err(completion_error), Err(pump_error)) => Err(completion_error.context(format!(
                "firmware MQTT pump join also failed: {pump_error:#}"
            ))),
        }
    }

    #[cfg(test)]
    pub(crate) async fn pause_pump_join_for_test(&mut self) -> FirmwareBarrierPauseHandle {
        let (pause, handle) = firmware_barrier_pause();
        self.pump
            .as_mut()
            .expect("firmware MQTT session has a pump")
            .pause_join_for_test(pause)
            .await;
        handle
    }

    #[cfg(test)]
    pub(crate) fn pause_shutdown_for_test(&mut self) -> FirmwareBarrierPauseHandle {
        let (pause, handle) = firmware_barrier_pause();
        self.shutdown_pause = Some(pause);
        handle
    }

    #[cfg(test)]
    pub(crate) fn fail_shutdown_completion_for_test(&mut self) {
        self.shutdown_completion_mode = ShutdownCompletionMode::Error;
    }

    #[cfg(test)]
    pub(crate) fn drop_shutdown_completion_for_test(&mut self) {
        self.shutdown_completion_mode = ShutdownCompletionMode::Drop;
    }

    #[cfg(test)]
    pub(crate) async fn panic_pump_for_test(&self) {
        let (reached, wait_reached) = oneshot::channel();
        self.requests
            .send(PumpRequest::Panic(reached))
            .await
            .expect("firmware pump accepts panic test request");
        wait_reached
            .await
            .expect("firmware pump reaches panic test request");
    }
}
