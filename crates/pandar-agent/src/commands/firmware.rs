use anyhow::{anyhow, bail};
use pandar_core::{FirmwareCommand as CoreFirmwareCommand, FirmwareTerminalOutcome};
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{
        FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest,
        FirmwareMachineGateway, FirmwarePrepareRequest, FirmwareRefreshRequest,
        proto_firmware_module, proto_upgrade_state,
    },
    protocol::agent::v1::{
        AgentEvent, ExecuteFirmwareControl, FirmwareAcknowledgement, FirmwareCommandResult,
        FirmwarePrepared, FirmwarePublished, FirmwareRefreshedModules, PrinterFirmwareStatus,
        PublishedWithoutAcknowledgement, agent_event, firmware_command, firmware_command_result,
        hub_command,
    },
};

use super::responses::firmware_result_event;
use super::{ack_event, events::event, failure_event, rejected_ack_event};

pub(crate) fn is_firmware_command(command: &crate::protocol::agent::v1::HubCommand) -> bool {
    matches!(
        command.command,
        Some(
            hub_command::Command::RefreshFirmwareVersion(_)
                | hub_command::Command::PrepareFirmwareControl(_)
                | hub_command::Command::ExecuteFirmwareControl(_)
        )
    )
}

pub(crate) async fn handle_firmware_command<G: FirmwareMachineGateway + ?Sized>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    outer_command_id: String,
    command: hub_command::Command,
    session_epoch: u64,
) -> anyhow::Result<()> {
    match command {
        hub_command::Command::RefreshFirmwareVersion(refresh) => {
            sender.send(ack_event(config, &outer_command_id)).await?;
            match gateway
                .refresh_firmware_version(FirmwareRefreshRequest {
                    serial: refresh.serial.clone(),
                    sequence_id: refresh.sequence_id,
                    expected_generation: refresh.expected_generation,
                })
                .await
            {
                Ok(mut delivery) => {
                    let observation = delivery.take_observation();
                    sender
                        .send(firmware_result_event(
                            config,
                            &outer_command_id,
                            FirmwareCommandResult {
                                command_id: outer_command_id.clone(),
                                serial: observation.serial,
                                generation: observation.generation,
                                transient_status: None,
                                outcome: Some(firmware_command_result::Outcome::RefreshedModules(
                                    FirmwareRefreshedModules {
                                        modules: observation
                                            .modules
                                            .into_iter()
                                            .map(proto_firmware_module)
                                            .collect(),
                                        module_revision: observation.revision,
                                    },
                                )),
                            },
                        ))
                        .await?;
                }
                Err(error) => {
                    sender
                        .send(failure_event(
                            config,
                            &outer_command_id,
                            format!("{error:#}"),
                        ))
                        .await?;
                }
            }
        }
        hub_command::Command::PrepareFirmwareControl(prepare) => {
            require_matching_id(&outer_command_id, &prepare.command_id)?;
            match gateway
                .prepare_firmware_control(FirmwarePrepareRequest {
                    command_id: prepare.command_id.clone(),
                    serial: prepare.serial.clone(),
                    expected_generation: prepare.expected_generation,
                    session_epoch,
                })
                .await
            {
                Ok(observation) => {
                    sender.send(ack_event(config, &outer_command_id)).await?;
                    sender
                        .send(event(
                            config,
                            "firmware-prepared",
                            agent_event::Event::FirmwarePrepared(FirmwarePrepared {
                                command_id: observation.command_id,
                                serial: observation.serial,
                                generation: observation.generation,
                            }),
                        ))
                        .await?;
                }
                Err(error) => {
                    sender
                        .send(rejected_ack_event(
                            config,
                            &outer_command_id,
                            format!("{error:#}"),
                        ))
                        .await?;
                }
            }
        }
        hub_command::Command::ExecuteFirmwareControl(execute) => {
            require_matching_id(&outer_command_id, &execute.command_id)?;
            let request = execute_request(execute, session_epoch)?;
            sender.send(ack_event(config, &outer_command_id)).await?;
            let serial = request.serial.clone();
            let generation = request.expected_generation;
            let (phases, mut phase_receiver) = mpsc::unbounded_channel();
            let operation = gateway.execute_firmware_control(request, phases);
            tokio::pin!(operation);
            let mut published_forwarded = false;
            let mut phases_open = true;
            let outcome = loop {
                tokio::select! {
                    result = &mut operation => break result,
                    phase = phase_receiver.recv(), if phases_open => match phase {
                        Some(phase) => forward_firmware_phase(
                            config,
                            sender,
                            &outer_command_id,
                            &serial,
                            generation,
                            phase,
                            &mut published_forwarded,
                        ).await?,
                        None => phases_open = false,
                    }
                }
            };
            while let Ok(phase) = phase_receiver.try_recv() {
                forward_firmware_phase(
                    config,
                    sender,
                    &outer_command_id,
                    &serial,
                    generation,
                    phase,
                    &mut published_forwarded,
                )
                .await?;
            }
            match outcome {
                Ok(outcome) => {
                    sender
                        .send(firmware_result_event(
                            config,
                            &outer_command_id,
                            proto_control_result(
                                outer_command_id.clone(),
                                serial,
                                generation,
                                outcome,
                            ),
                        ))
                        .await?;
                }
                Err(error) => {
                    sender
                        .send(failure_event(
                            config,
                            &outer_command_id,
                            format!("{error:#}"),
                        ))
                        .await?;
                }
            }
        }
        _ => bail!("non-firmware command reached firmware handler"),
    }
    Ok(())
}

async fn forward_firmware_phase(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    serial: &str,
    generation: u64,
    phase: FirmwareControlPhase,
    published_forwarded: &mut bool,
) -> anyhow::Result<()> {
    match phase {
        FirmwareControlPhase::Published if !*published_forwarded => {
            sender
                .send(event(
                    config,
                    "firmware-published",
                    agent_event::Event::FirmwarePublished(FirmwarePublished {
                        command_id: command_id.to_owned(),
                        serial: serial.to_owned(),
                        generation,
                    }),
                ))
                .await?;
            *published_forwarded = true;
        }
        FirmwareControlPhase::Published => {}
    }
    Ok(())
}

fn require_matching_id(outer: &str, inner: &str) -> anyhow::Result<()> {
    if outer != inner {
        bail!("firmware outer command id does not match inner command id");
    }
    Ok(())
}

fn execute_request(
    execute: ExecuteFirmwareControl,
    session_epoch: u64,
) -> anyhow::Result<FirmwareExecuteRequest> {
    let command = execute
        .command
        .ok_or_else(|| anyhow!("execute firmware control is missing command"))?;
    let command = match command
        .command
        .ok_or_else(|| anyhow!("execute firmware control is missing command variant"))?
    {
        firmware_command::Command::UpgradeConfirm(_) => CoreFirmwareCommand::UpgradeConfirm {
            sequence_id: command.sequence_id,
            src_id: command.src_id,
        },
        firmware_command::Command::ConsistencyConfirm(_) => {
            CoreFirmwareCommand::ConsistencyConfirm {
                sequence_id: command.sequence_id,
                src_id: command.src_id,
            }
        }
        firmware_command::Command::Start(start) => CoreFirmwareCommand::Start {
            sequence_id: command.sequence_id,
            src_id: command.src_id,
            url: start.url,
            module: start.module,
            version: start.version,
        },
        firmware_command::Command::SwitchAmsFirmware(switch) => {
            CoreFirmwareCommand::SwitchAmsFirmware {
                sequence_id: command.sequence_id,
                src_id: command.src_id,
                id: switch.id,
            }
        }
    };
    Ok(FirmwareExecuteRequest {
        command_id: execute.command_id,
        serial: execute.serial,
        expected_generation: execute.expected_generation,
        session_epoch,
        command,
    })
}

fn proto_control_result(
    command_id: String,
    serial: String,
    generation: u64,
    outcome: FirmwareControlOutcome,
) -> FirmwareCommandResult {
    let transient_status = outcome
        .transient_status
        .map(|status| PrinterFirmwareStatus {
            upgrade_state: status.upgrade_state.map(proto_upgrade_state),
            cfg: status.cfg,
        });
    let outcome = match outcome.terminal {
        FirmwareTerminalOutcome::Acknowledged { acknowledgement } => {
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: acknowledgement.command,
                sequence_id: acknowledgement.sequence_id,
                result: acknowledgement.result,
                error_code: acknowledgement.error_code,
                reason: acknowledgement.reason,
                message: acknowledgement.message,
            })
        }
        FirmwareTerminalOutcome::PublishedWithoutAcknowledgement => {
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(
                PublishedWithoutAcknowledgement {},
            )
        }
    };
    FirmwareCommandResult {
        command_id,
        serial,
        generation,
        transient_status,
        outcome: Some(outcome),
    }
}
