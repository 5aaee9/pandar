use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{
    FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest, FirmwareModulesDelivery,
    FirmwarePrepareRequest, FirmwarePreparedObservation, FirmwareRefreshRequest,
};

#[async_trait]
pub trait FirmwareMachineGateway: Send + Sync {
    async fn refresh_firmware_version(
        &self,
        request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery>;

    async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation>;

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome>;

    async fn cancel_firmware_session(&self, session_epoch: u64) -> anyhow::Result<()>;
}
