use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, Printer, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel,
    QueryFilter, QuerySelect,
};

use super::{PrinterRepository, PrinterSnapshotUpsert, upsert_snapshot_in_transaction};
use crate::{
    entities::printers,
    repositories::{RepositoryError, RepositoryResult, begin_current_agent_transaction},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFeatureUpdateOutcome {
    Updated,
    StaleOrMissing,
}

impl PrinterRepository {
    pub async fn upsert_snapshot_with_device_features_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        snapshot: PrinterSnapshotUpsert,
        features: Option<BambuDeviceFeatures>,
    ) -> RepositoryResult<Printer> {
        let transaction =
            begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
                .await?;
        let feature_session_id = features.map(|_| session_id);
        let printer = upsert_snapshot_in_transaction(
            &transaction,
            tenant_id,
            agent_id,
            snapshot,
            features,
            feature_session_id,
        )
        .await?;
        transaction
            .commit()
            .await
            .context("failed to commit current-session printer snapshot with device features")?;
        Ok(printer)
    }

    pub async fn update_device_features_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        serial: &str,
        features: Option<BambuDeviceFeatures>,
    ) -> RepositoryResult<DeviceFeatureUpdateOutcome> {
        let transaction =
            match begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
                .await
            {
                Ok(transaction) => transaction,
                Err(RepositoryError::AgentSessionNotCurrent | RepositoryError::MissingAgent) => {
                    return Ok(DeviceFeatureUpdateOutcome::StaleOrMissing);
                }
                Err(error) => return Err(error),
            };
        let query = printers::Entity::find()
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printers::Column::AgentId.eq(agent_id.to_string()))
            .filter(printers::Column::SerialNumber.eq(serial));
        let printer = match transaction.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(&transaction).await,
            _ => query.one(&transaction).await,
        }
        .context("failed to lock printer for Bambu device feature update")?;
        let Some(printer) = printer else {
            transaction
                .commit()
                .await
                .context("failed to commit missing Bambu device feature update")?;
            return Ok(DeviceFeatureUpdateOutcome::StaleOrMissing);
        };

        let mut active = printer.into_active_model();
        match features {
            Some(features) => {
                active.bambu_fun_bits = Set(Some(features.to_hex()));
                active.bambu_fun_session_id = Set(Some(session_id.to_owned()));
            }
            None => active.bambu_fun_session_id = Set(None),
        }
        active
            .update(&transaction)
            .await
            .context("failed to persist Bambu device feature update")?;
        transaction
            .commit()
            .await
            .context("failed to commit Bambu device feature update")?;

        Ok(DeviceFeatureUpdateOutcome::Updated)
    }
}
