use crate::machine::mqtt::FirmwarePumpAbortHandle;

pub(super) struct FirmwarePumpCancellationGuard {
    pump_abort: Option<FirmwarePumpAbortHandle>,
}

impl FirmwarePumpCancellationGuard {
    pub(super) fn new(pump_abort: FirmwarePumpAbortHandle) -> Self {
        Self {
            pump_abort: Some(pump_abort),
        }
    }

    pub(super) fn disarm(mut self) {
        self.pump_abort.take();
    }
}

impl Drop for FirmwarePumpCancellationGuard {
    fn drop(&mut self) {
        if let Some(pump_abort) = self.pump_abort.take() {
            pump_abort.abort();
        }
    }
}
