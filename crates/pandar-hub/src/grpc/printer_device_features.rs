use pandar_core::{AgentId, BambuDeviceFeatures, TenantId};
use tonic::Status;

use crate::{
    AppState, grpc::commands::repository_status,
    protocol::agent::v1::PrinterDeviceFeaturesSnapshot, sessions::SessionToken,
};

pub(super) async fn handle_device_features_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterDeviceFeaturesSnapshot,
) -> Result<(), Status> {
    let serial = snapshot.serial.trim();
    if serial.is_empty() {
        return Err(Status::invalid_argument("serial must not be blank"));
    }
    let serial = serial.to_owned();
    let features = snapshot
        .device_features
        .map(|features| BambuDeviceFeatures::from_bits(features.bambu_fun_bits));

    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    state
        .printers()
        .update_device_features_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            &serial,
            features,
        )
        .await
        .map_err(repository_status)?;

    Ok(())
}
