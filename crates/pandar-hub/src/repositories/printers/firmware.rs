use anyhow::Context;
use pandar_core::{
    AgentId, PrinterFirmwareModule, PrinterFirmwareState, PrinterUpgradeState, TenantId,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseTransaction, EntityTrait,
    IntoActiveModel, QueryFilter,
};

use super::PrinterRepository;
use crate::db::ConnectionDialectExt;
use crate::{
    entities::printers,
    repositories::{RepositoryError, RepositoryResult, begin_current_agent_transaction},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrinterFirmwareUpdateOutcome {
    Applied,
    Stale,
}

impl PrinterRepository {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn begin_firmware_dispatch_fence(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        printer_id: &str,
        serial: &str,
        generation: u64,
    ) -> RepositoryResult<Option<DatabaseTransaction>> {
        let generation = signed_value(generation, "firmware generation")?;
        let transaction =
            match begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
                .await
            {
                Ok(transaction) => transaction,
                Err(RepositoryError::AgentSessionNotCurrent | RepositoryError::MissingAgent) => {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
        let printer = lock_printer_by_id(&transaction, tenant_id, printer_id).await?;
        let current = printer.is_some_and(|printer| {
            printer.agent_id == agent_id.to_string()
                && printer.serial_number == serial
                && printer.firmware_session_id.as_deref() == Some(session_id)
                && printer.firmware_generation == Some(generation)
        });
        if !current {
            transaction
                .commit()
                .await
                .context("failed to commit stale firmware dispatch fence")?;
            return Ok(None);
        }
        Ok(Some(transaction))
    }

    pub async fn establish_generation_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        serial: &str,
        generation: u64,
    ) -> RepositoryResult<PrinterFirmwareUpdateOutcome> {
        let generation = signed_value(generation, "firmware generation")?;
        let Some(transaction) = self
            .begin_firmware_update(tenant_id, agent_id, session_id)
            .await?
        else {
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        let Some(printer) = lock_printer(&transaction, tenant_id, agent_id, serial).await? else {
            transaction
                .commit()
                .await
                .context("failed to commit missing firmware generation update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        let is_new_session = printer.firmware_session_id.as_deref() != Some(session_id);
        let is_newer_generation = printer
            .firmware_generation
            .is_none_or(|current| generation > current);
        if !is_new_session && !is_newer_generation {
            transaction
                .commit()
                .await
                .context("failed to commit stale firmware generation update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        }

        let mut active = printer.into_active_model();
        active.firmware_modules_json = Set(None);
        active.firmware_upgrade_state_json = Set(None);
        active.firmware_cfg = Set(None);
        active.firmware_session_id = Set(Some(session_id.to_owned()));
        active.firmware_generation = Set(Some(generation));
        active.firmware_module_revision = Set(0);
        active.firmware_status_revision = Set(0);
        active
            .update(&transaction)
            .await
            .context("failed to persist printer firmware generation")?;
        transaction
            .commit()
            .await
            .context("failed to commit printer firmware generation")?;
        Ok(PrinterFirmwareUpdateOutcome::Applied)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn replace_modules_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        serial: &str,
        generation: u64,
        revision: u64,
        modules: Vec<PrinterFirmwareModule>,
    ) -> RepositoryResult<PrinterFirmwareUpdateOutcome> {
        let generation = signed_value(generation, "firmware generation")?;
        let revision = signed_value(revision, "firmware module revision")?;
        let modules_json = serde_json::to_string(&modules)
            .context("failed to serialize printer firmware modules")?;
        let Some(transaction) = self
            .begin_firmware_update(tenant_id, agent_id, session_id)
            .await?
        else {
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        let Some(printer) = lock_printer(&transaction, tenant_id, agent_id, serial).await? else {
            transaction
                .commit()
                .await
                .context("failed to commit missing firmware module update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        if printer.firmware_session_id.as_deref() != Some(session_id)
            || printer.firmware_generation != Some(generation)
            || revision <= printer.firmware_module_revision
        {
            transaction
                .commit()
                .await
                .context("failed to commit stale firmware module update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        }

        let mut active = printer.into_active_model();
        active.firmware_modules_json = Set(Some(modules_json));
        active.firmware_module_revision = Set(revision);
        active
            .update(&transaction)
            .await
            .context("failed to persist printer firmware modules")?;
        transaction
            .commit()
            .await
            .context("failed to commit printer firmware modules")?;
        Ok(PrinterFirmwareUpdateOutcome::Applied)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn replace_status_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        serial: &str,
        generation: u64,
        revision: u64,
        state: Option<PrinterUpgradeState>,
        cfg: Option<String>,
    ) -> RepositoryResult<PrinterFirmwareUpdateOutcome> {
        let generation = signed_value(generation, "firmware generation")?;
        let revision = signed_value(revision, "firmware status revision")?;
        let state_json = state
            .map(|state| {
                serde_json::to_string(&state)
                    .context("failed to serialize printer firmware upgrade state")
            })
            .transpose()?;
        let Some(transaction) = self
            .begin_firmware_update(tenant_id, agent_id, session_id)
            .await?
        else {
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        let Some(printer) = lock_printer(&transaction, tenant_id, agent_id, serial).await? else {
            transaction
                .commit()
                .await
                .context("failed to commit missing firmware status update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        };
        if printer.firmware_session_id.as_deref() != Some(session_id)
            || printer.firmware_generation != Some(generation)
            || revision <= printer.firmware_status_revision
        {
            transaction
                .commit()
                .await
                .context("failed to commit stale firmware status update")?;
            return Ok(PrinterFirmwareUpdateOutcome::Stale);
        }

        let mut active = printer.into_active_model();
        active.firmware_upgrade_state_json = Set(state_json);
        active.firmware_cfg = Set(cfg);
        active.firmware_status_revision = Set(revision);
        active
            .update(&transaction)
            .await
            .context("failed to persist printer firmware status")?;
        transaction
            .commit()
            .await
            .context("failed to commit printer firmware status")?;
        Ok(PrinterFirmwareUpdateOutcome::Applied)
    }

    async fn begin_firmware_update(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
    ) -> RepositoryResult<Option<DatabaseTransaction>> {
        match begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id).await
        {
            Ok(transaction) => Ok(Some(transaction)),
            Err(RepositoryError::AgentSessionNotCurrent | RepositoryError::MissingAgent) => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

pub(crate) fn from_model(model: &printers::Model) -> RepositoryResult<PrinterFirmwareState> {
    (|| -> anyhow::Result<PrinterFirmwareState> {
        Ok(PrinterFirmwareState {
            session_id: model.firmware_session_id.clone(),
            generation: model
                .firmware_generation
                .map(u64::try_from)
                .transpose()
                .context("failed to read printer firmware generation")?,
            module_revision: u64::try_from(model.firmware_module_revision)
                .context("failed to read printer firmware module revision")?,
            status_revision: u64::try_from(model.firmware_status_revision)
                .context("failed to read printer firmware status revision")?,
            modules: model
                .firmware_modules_json
                .as_deref()
                .map(|value| {
                    serde_json::from_str(value).context("failed to read printer firmware modules")
                })
                .transpose()?,
            upgrade_state: model
                .firmware_upgrade_state_json
                .as_deref()
                .map(|value| {
                    serde_json::from_str(value)
                        .context("failed to read printer firmware upgrade state")
                })
                .transpose()?,
            cfg: model.firmware_cfg.clone(),
        })
    })()
    .context("failed to rehydrate printer firmware")
    .map_err(RepositoryError::from)
}

async fn lock_printer(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: &str,
) -> RepositoryResult<Option<printers::Model>> {
    let query = printers::Entity::find()
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(agent_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(serial));
    transaction
        .lock_for_update(query)
        .one(transaction)
        .await
        .context("failed to lock printer firmware row")
        .map_err(Into::into)
}

async fn lock_printer_by_id(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    printer_id: &str,
) -> RepositoryResult<Option<printers::Model>> {
    let query = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()));
    transaction
        .lock_for_update(query)
        .one(transaction)
        .await
        .context("failed to lock firmware dispatch printer row")
        .map_err(Into::into)
}

fn signed_value(value: u64, name: &'static str) -> RepositoryResult<i64> {
    i64::try_from(value)
        .with_context(|| format!("{name} exceeds i64::MAX"))
        .map_err(Into::into)
}
