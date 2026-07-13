use super::*;

impl<T, F> TestRuntimeBambuMachineGateway<T, F> {
    pub(crate) fn firmware_cache(&self) -> FirmwareObservationCache {
        self.firmware.clone()
    }

    pub(crate) async fn set_refresh_context(
        &self,
        config: AgentConfig,
        sender: mpsc::Sender<AgentEvent>,
    ) {
        *self.refresh_context.lock().await = Some((config, sender));
    }
}
