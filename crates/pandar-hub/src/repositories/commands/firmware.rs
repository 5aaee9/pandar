use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, CommandRecord, CommandStatus, FirmwareControlMetadata,
    FirmwareTerminalOutcome, PrinterFirmwareStatus, TenantId,
};
use sea_orm::{ConnectionTrait, DatabaseTransaction, EntityTrait, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::{
    db::Database,
    entities::commands,
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
        commands::{
            CommandRepository, TerminalCommandTransition,
            audit::get_command,
            inserts::{self, InsertCommand},
            ownership,
            rows::command_from_model,
            transitions::{self, StatusTransition},
        },
    },
};

impl CommandRepository {
    pub async fn create_firmware_refresh_sent_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        expected_agent_id: AgentId,
        owner: FirmwareCommandOwner,
        sequence_id: String,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        create_refresh_sent_with_audit(
            &self.database,
            tenant_id,
            printer_id,
            expected_agent_id,
            owner,
            sequence_id,
            actor,
        )
        .await
    }

    pub async fn create_firmware_control_sent_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        expected_agent_id: AgentId,
        owner: FirmwareCommandOwner,
        metadata: FirmwareControlMetadata,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        create_control_sent_with_audit(
            &self.database,
            tenant_id,
            printer_id,
            expected_agent_id,
            owner,
            metadata,
            actor,
        )
        .await
    }

    pub async fn mark_firmware_execute_sent(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        let transaction = self
            .database
            .sea_orm_connection()
            .begin()
            .await
            .context("failed to begin firmware execute phase transaction")?;
        let command = self
            .mark_firmware_execute_sent_on(&transaction, command_id, tenant_id, agent_id)
            .await?;
        transaction
            .commit()
            .await
            .context("failed to commit firmware execute phase transaction")?;
        Ok(command)
    }
    pub(crate) async fn mark_firmware_execute_sent_on(
        &self,
        transaction: &DatabaseTransaction,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        let command = require_kind(
            load_owned_on(transaction, command_id, tenant_id, agent_id).await?,
            "firmware_control",
        )?;
        if command.status == CommandStatus::Acknowledged {
            return Ok(command);
        }
        let updated = transitions::update_status_if_current_on(
            transaction,
            StatusTransition {
                command_id,
                tenant_id,
                agent_id,
                status: CommandStatus::Acknowledged,
                error: None,
                result_json: None,
                allowed_statuses: &[CommandStatus::Sent],
            },
        )
        .await?;
        let command = require_kind(
            load_owned_on(transaction, command_id, tenant_id, agent_id).await?,
            "firmware_control",
        )?;
        if updated || command.status == CommandStatus::Acknowledged {
            Ok(command)
        } else {
            Err(RepositoryError::InvalidCommandTransition {
                from: command.status.as_str().to_owned(),
                action: "advance firmware command",
            })
        }
    }

    pub async fn mark_firmware_terminal(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        status: CommandStatus,
        error: Option<String>,
        result: FirmwarePersistedResult,
    ) -> RepositoryResult<CommandRecord> {
        let command = self.load_owned(command_id, tenant_id, agent_id).await?;
        if !matches!(
            command.kind.as_str(),
            "firmware_refresh" | "firmware_control"
        ) {
            return Err(RepositoryError::InvalidCommandTransition {
                from: command.status.as_str().to_owned(),
                action: "finish firmware command",
            });
        }
        let result_json = serde_json::to_string(&result)
            .context("failed to serialize typed firmware command result")?;
        let command = self
            .guard_terminal_transition(TerminalCommandTransition {
                command_id,
                tenant_id,
                agent_id,
                terminal_status: status.clone(),
                error: error.clone(),
                result_json: Some(result_json.clone()),
                action: "finish firmware command",
            })
            .await?;
        if command.status == status
            && command.error == error
            && command.result_json.as_deref() == Some(result_json.as_str())
        {
            Ok(command)
        } else {
            Err(RepositoryError::InvalidCommandTransition {
                from: command.status.as_str().to_owned(),
                action: "finish firmware command with a different terminal result",
            })
        }
    }
}

async fn load_owned_on<C>(
    connection: &C,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let command = commands::Entity::find_by_id(command_id.to_string())
        .one(connection)
        .await
        .context("failed to load firmware command in session fence")?
        .map(command_from_model)
        .transpose()?
        .ok_or(RepositoryError::MissingCommand)?;
    if command.tenant_id != tenant_id || command.agent_id != agent_id {
        return Err(RepositoryError::CommandOwnershipMismatch);
    }
    Ok(command)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwareCommandOwner {
    pub session_id: String,
    pub instance_id: uuid::Uuid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwareRefreshPayload {
    pub action: String,
    pub serial: String,
    pub owner_session_id: String,
    pub owner_instance_id: uuid::Uuid,
    pub sequence_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwareControlPayload {
    pub serial: String,
    pub owner_session_id: String,
    pub owner_instance_id: uuid::Uuid,
    pub command: FirmwareControlMetadata,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwarePersistedPhase {
    PrePublishFailure,
    Acknowledged,
    Rejected,
    OutcomeUnknown,
    Refreshed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwarePersistedResult {
    pub phase: FirmwarePersistedPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<FirmwareTerminalOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transient_status: Option<PrinterFirmwareStatus>,
}

#[derive(Serialize)]
struct FirmwareRefreshAudit<'a> {
    agent_id: String,
    serial: &'a str,
    sequence_id: &'a str,
}

#[derive(Serialize)]
struct FirmwareControlAudit<'a> {
    agent_id: String,
    serial: &'a str,
    command: &'a FirmwareControlMetadata,
}

pub async fn create_refresh_sent_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    expected_agent_id: AgentId,
    owner: FirmwareCommandOwner,
    sequence_id: String,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin firmware refresh audit transaction")?;
    ownership::lock_agent_owner_on(&tx, tenant_id, expected_agent_id).await?;
    let printer =
        ownership::locked_printer_for_expected_agent(&tx, tenant_id, printer_id, expected_agent_id)
            .await?;
    let payload = FirmwareRefreshPayload {
        action: "refresh_version".to_owned(),
        serial: printer.serial_number.clone(),
        owner_session_id: owner.session_id,
        owner_instance_id: owner.instance_id,
        sequence_id,
    };
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize firmware refresh command payload")?;
    let command_id = pandar_core::CommandId::new();
    let now = pandar_core::created_at_now();
    inserts::insert_with_status(
        &tx,
        InsertCommand {
            id: command_id,
            tenant_id,
            agent_id: expected_agent_id,
            printer_id: Some(&printer.id),
            kind: "firmware_refresh",
            payload_json: &payload_json,
            created_at: &now,
        },
        CommandStatus::Sent,
    )
    .await?;
    insert_audit_event_tx(
        &tx,
        &record_audit_event(
            tenant_id,
            actor,
            "printer.firmware_refresh",
            "printer",
            Some(printer.id),
            audit_metadata(FirmwareRefreshAudit {
                agent_id: expected_agent_id.to_string(),
                serial: &printer.serial_number,
                sequence_id: &payload.sequence_id,
            }),
        ),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit firmware refresh audit transaction")?;
    get_command(database, command_id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub async fn create_control_sent_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    expected_agent_id: AgentId,
    owner: FirmwareCommandOwner,
    metadata: FirmwareControlMetadata,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin firmware control audit transaction")?;
    ownership::lock_agent_owner_on(&tx, tenant_id, expected_agent_id).await?;
    let printer =
        ownership::locked_printer_for_expected_agent(&tx, tenant_id, printer_id, expected_agent_id)
            .await?;
    let payload = FirmwareControlPayload {
        serial: printer.serial_number.clone(),
        owner_session_id: owner.session_id,
        owner_instance_id: owner.instance_id,
        command: metadata,
    };
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize firmware control command payload")?;
    let command_id = pandar_core::CommandId::new();
    let now = pandar_core::created_at_now();
    inserts::insert_with_status(
        &tx,
        InsertCommand {
            id: command_id,
            tenant_id,
            agent_id: expected_agent_id,
            printer_id: Some(&printer.id),
            kind: "firmware_control",
            payload_json: &payload_json,
            created_at: &now,
        },
        CommandStatus::Sent,
    )
    .await?;
    insert_audit_event_tx(
        &tx,
        &record_audit_event(
            tenant_id,
            actor,
            "printer.firmware_control",
            "printer",
            Some(printer.id),
            audit_metadata(FirmwareControlAudit {
                agent_id: expected_agent_id.to_string(),
                serial: &printer.serial_number,
                command: &payload.command,
            }),
        ),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit firmware control audit transaction")?;
    get_command(database, command_id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub fn require_kind(
    command: CommandRecord,
    expected: &'static str,
) -> RepositoryResult<CommandRecord> {
    if command.kind == expected {
        Ok(command)
    } else {
        Err(RepositoryError::InvalidCommandTransition {
            from: command.status.as_str().to_owned(),
            action: "advance firmware command",
        })
    }
}
