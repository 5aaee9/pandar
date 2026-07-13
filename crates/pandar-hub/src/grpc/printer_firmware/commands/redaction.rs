use super::*;

pub(super) fn redact_result_strings(
    state: &AppState,
    identity: &FirmwareCommandIdentity,
    result: &mut FirmwareCommandResult,
) {
    let mut redact = |value: &mut String| {
        *value = state
            .sessions()
            .redact_firmware_text_under_transition(identity, value);
    };
    redact(&mut result.command_id);
    redact(&mut result.serial);
    if let Some(status) = &mut result.transient_status {
        redact_optional(&mut status.cfg, &mut redact);
        if let Some(upgrade) = &mut status.upgrade_state {
            for value in [
                &mut upgrade.status,
                &mut upgrade.progress,
                &mut upgrade.message,
                &mut upgrade.module,
                &mut upgrade.ota_new_version_number,
                &mut upgrade.ams_new_version_number,
                &mut upgrade.ahb_new_version_number,
            ] {
                redact_optional(value, &mut redact);
            }
            if let Some(versions) = &mut upgrade.new_versions {
                for version in &mut versions.versions {
                    redact(&mut version.name);
                    redact_optional(&mut version.current_version, &mut redact);
                    redact_optional(&mut version.new_version, &mut redact);
                }
            }
            if let Some(ams) = &mut upgrade.ams_firmware {
                redact_optional(&mut ams.status, &mut redact);
                if let Some(firmware) = &mut ams.firmware {
                    for entry in &mut firmware.firmware {
                        redact(&mut entry.name);
                        redact(&mut entry.version);
                    }
                }
            }
        }
    }
    if let Some(outcome) = &mut result.outcome {
        match outcome {
            firmware_command_result::Outcome::RefreshedModules(refreshed) => {
                for module in &mut refreshed.modules {
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
            firmware_command_result::Outcome::Acknowledgement(acknowledgement) => {
                redact(&mut acknowledgement.command);
                redact(&mut acknowledgement.sequence_id);
                for value in [
                    &mut acknowledgement.result,
                    &mut acknowledgement.reason,
                    &mut acknowledgement.message,
                ] {
                    redact_optional(value, &mut redact);
                }
            }
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(_) => {}
        }
    }
}

fn redact_optional(value: &mut Option<String>, redact: &mut impl FnMut(&mut String)) {
    if let Some(value) = value {
        redact(value);
    }
}
