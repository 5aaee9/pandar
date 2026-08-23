use pandar_core::{
    AgentId, AmsFirmwareDescriptor, AmsFirmwareSwitchState,
    PrinterFirmwareModule as CoreFirmwareModule, PrinterFirmwareVersion,
    PrinterUpgradeState as CoreUpgradeState, TenantId,
};
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::repository_status,
    protocol::agent::v1::{
        PrinterFirmwareInvalidated, PrinterFirmwareModule, PrinterFirmwareModulesSnapshot,
        PrinterFirmwareStatusSnapshot, PrinterUpgradeState,
    },
    repositories::PrinterFirmwareUpdateOutcome,
    sessions::SessionToken,
};

mod commands;
#[cfg(test)]
pub(crate) use commands::completion_pause;
pub(super) use commands::{
    handle_command_ack, handle_command_failure, handle_command_result, handle_prepared,
    handle_published,
};

pub(super) async fn handle_invalidated(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    invalidated: PrinterFirmwareInvalidated,
) -> Result<(), Status> {
    let serial = required_serial(&invalidated.serial)?;
    let generation = storage_value(invalidated.generation, "generation")?;
    let lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let outcome = state
        .printers()
        .establish_generation_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            &serial,
            generation,
        )
        .await
        .map_err(repository_status)?;
    if outcome == PrinterFirmwareUpdateOutcome::Stale {
        return Ok(());
    }
    let cancelled = state
        .sessions()
        .cancel_firmware_generation_under_transition(agent_id, token, &serial, generation);
    drop(lease);
    crate::firmware_control::finish_cancelled_commands(
        state,
        cancelled,
        "firmware observation generation changed after command dispatch",
    )
    .await;
    if outcome == PrinterFirmwareUpdateOutcome::Applied {
        state
            .publish_printer_projection_change_for_serial(tenant_id, &serial)
            .await;
    }
    Ok(())
}

pub(super) async fn handle_modules_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterFirmwareModulesSnapshot,
) -> Result<(), Status> {
    let serial = required_serial(&snapshot.serial)?;
    let generation = storage_value(snapshot.generation, "generation")?;
    let revision = storage_value(snapshot.module_revision, "module_revision")?;
    if !module_names_are_valid(&snapshot.modules) {
        return Err(Status::invalid_argument(
            "firmware module name must not be empty",
        ));
    }
    let mut modules = snapshot
        .modules
        .into_iter()
        .map(core_module)
        .collect::<Vec<_>>();
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    redact_modules_snapshot(state, tenant_id, &serial, &mut modules);
    let outcome = state
        .printers()
        .replace_modules_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            &serial,
            generation,
            revision,
            modules,
        )
        .await
        .map_err(repository_status)?;
    if outcome == PrinterFirmwareUpdateOutcome::Applied {
        state
            .publish_printer_projection_change_for_serial(tenant_id, &serial)
            .await;
    }
    Ok(())
}

pub(super) async fn handle_status_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterFirmwareStatusSnapshot,
) -> Result<(), Status> {
    let serial = required_serial(&snapshot.serial)?;
    let generation = storage_value(snapshot.generation, "generation")?;
    let revision = storage_value(snapshot.status_revision, "status_revision")?;
    let mut upgrade_state = snapshot.upgrade_state.map(core_upgrade_state);
    let mut cfg = snapshot.cfg;
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let mut redact = |value: &mut String| {
        *value = state
            .sessions()
            .redact_firmware_snapshot_text_under_transition(tenant_id, &serial, value);
    };
    redact_optional(&mut cfg, &mut redact);
    if let Some(upgrade_state) = &mut upgrade_state {
        redact_upgrade_state(upgrade_state, &mut redact);
    }
    let outcome = state
        .printers()
        .replace_status_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            &serial,
            generation,
            revision,
            upgrade_state,
            cfg,
        )
        .await
        .map_err(repository_status)?;
    if outcome == PrinterFirmwareUpdateOutcome::Applied {
        state
            .publish_printer_projection_change_for_serial(tenant_id, &serial)
            .await;
    }
    Ok(())
}

fn required_serial(serial: &str) -> Result<String, Status> {
    let serial = serial.trim();
    if serial.is_empty() {
        return Err(Status::invalid_argument("serial must not be blank"));
    }
    Ok(serial.to_owned())
}

fn storage_value(value: u64, name: &'static str) -> Result<u64, Status> {
    i64::try_from(value)
        .map(|_| value)
        .map_err(|_| Status::invalid_argument(format!("{name} must be at most i64::MAX")))
}

fn core_module(module: PrinterFirmwareModule) -> CoreFirmwareModule {
    CoreFirmwareModule {
        name: module.name,
        software_version: module.software_version,
        software_new_version: module.software_new_version,
        new_version: module.new_version,
        visible: module.visible,
        product_name: module.product_name,
        serial_number: module.serial_number,
        hardware_version: module.hardware_version,
        firmware_flag: module.firmware_flag,
    }
}

fn module_names_are_valid(modules: &[PrinterFirmwareModule]) -> bool {
    modules.iter().all(|module| !module.name.is_empty())
}

fn core_upgrade_state(state: PrinterUpgradeState) -> CoreUpgradeState {
    CoreUpgradeState {
        status: state.status,
        progress: state.progress,
        message: state.message,
        module: state.module,
        error_code: state.error_code,
        new_version_state: state.new_version_state,
        consistency_request: state.consistency_request,
        force_upgrade: state.force_upgrade,
        display_state: state.display_state,
        ota_new_version_number: state.ota_new_version_number,
        ams_new_version_number: state.ams_new_version_number,
        ahb_new_version_number: state.ahb_new_version_number,
        new_versions: state.new_versions.map(|versions| {
            versions
                .versions
                .into_iter()
                .map(|version| PrinterFirmwareVersion {
                    name: version.name,
                    current_version: version.current_version,
                    new_version: version.new_version,
                })
                .collect()
        }),
        ams_firmware: state.ams_firmware.map(|ams| AmsFirmwareSwitchState {
            firmware: ams.firmware.map(|firmware| {
                firmware
                    .firmware
                    .into_iter()
                    .map(|entry| AmsFirmwareDescriptor {
                        id: entry.id,
                        name: entry.name,
                        version: entry.version,
                    })
                    .collect()
            }),
            current_firmware_id: ams.current_firmware_id,
            current_run_firmware_id: ams.current_run_firmware_id,
            status: ams.status,
        }),
    }
}

fn redact_modules_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    serial: &str,
    modules: &mut [CoreFirmwareModule],
) {
    let mut redact = |value: &mut String| {
        *value = state
            .sessions()
            .redact_firmware_snapshot_text_under_transition(tenant_id, serial, value);
    };
    for module in modules {
        redact(&mut module.name);
        for value in [
            &mut module.software_version,
            &mut module.software_new_version,
            &mut module.new_version,
            &mut module.product_name,
            &mut module.serial_number,
            &mut module.hardware_version,
        ] {
            redact_optional(value, &mut redact);
        }
    }
}

fn redact_upgrade_state(upgrade: &mut CoreUpgradeState, redact: &mut impl FnMut(&mut String)) {
    for value in [
        &mut upgrade.status,
        &mut upgrade.progress,
        &mut upgrade.message,
        &mut upgrade.module,
        &mut upgrade.ota_new_version_number,
        &mut upgrade.ams_new_version_number,
        &mut upgrade.ahb_new_version_number,
    ] {
        redact_optional(value, redact);
    }
    if let Some(versions) = &mut upgrade.new_versions {
        for version in versions {
            redact(&mut version.name);
            redact_optional(&mut version.current_version, redact);
            redact_optional(&mut version.new_version, redact);
        }
    }
    if let Some(ams) = &mut upgrade.ams_firmware {
        redact_optional(&mut ams.status, redact);
        if let Some(firmware) = &mut ams.firmware {
            for entry in firmware {
                redact(&mut entry.name);
                redact(&mut entry.version);
            }
        }
    }
}

fn redact_optional(value: &mut Option<String>, redact: &mut impl FnMut(&mut String)) {
    if let Some(value) = value {
        redact(value);
    }
}
