use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, Printer, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};

use super::{
    PrinterRepository, PrinterSnapshotUpsert, SnapshotSessionState, printer_from_model,
    upsert_snapshot_in_transaction,
};
use crate::db::ConnectionDialectExt;
use crate::{
    entities::{printer_material_snapshots, printers},
    repositories::{RepositoryError, RepositoryResult, begin_current_agent_transaction},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFeatureUpdateOutcome {
    Updated,
    Unchanged,
    StaleOrMissing,
}

impl PrinterRepository {
    pub async fn apply_snapshot_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        snapshot: PrinterSnapshotUpsert,
        features: Option<BambuDeviceFeatures>,
        secondary_features: Option<BambuDeviceFeatures>,
    ) -> RepositoryResult<Printer> {
        let clear_materials = snapshot.connection_authoritative;
        let transaction =
            begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
                .await?;
        let feature_session_id = features.map(|_| session_id);
        let nozzle_system_session_id = snapshot.nozzle_system.as_ref().map(|_| session_id);
        let presence_session_id = snapshot.telemetry_authoritative.then_some(session_id);
        let printer = upsert_snapshot_in_transaction(
            &transaction,
            tenant_id,
            agent_id,
            snapshot,
            SnapshotSessionState {
                device_features: features,
                device_features_session_id: feature_session_id,
                nozzle_system_session_id,
                mqtt_presence_session_id: presence_session_id,
            },
            &self.access_code_cipher,
        )
        .await?;
        if let Some(features) = secondary_features {
            printers::Entity::update_many()
                .set(printers::ActiveModel {
                    bambu_fun2_bits: Set(Some(features.to_hex())),
                    bambu_fun2_session_id: Set(Some(session_id.to_owned())),
                    ..Default::default()
                })
                .filter(printers::Column::Id.eq(&printer.id))
                .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
                .filter(printers::Column::AgentId.eq(agent_id.to_string()))
                .exec(&transaction)
                .await
                .context("failed to update secondary Bambu device features in printer snapshot")?;
        }
        if clear_materials {
            printer_material_snapshots::Entity::delete_many()
                .filter(printer_material_snapshots::Column::TenantId.eq(tenant_id.to_string()))
                .filter(printer_material_snapshots::Column::PrinterId.eq(&printer.id))
                .exec(&transaction)
                .await
                .context("failed to clear materials in authoritative printer snapshot")?;
        }
        let printer = printers::Entity::find_by_id(&printer.id)
            .one(&transaction)
            .await
            .context("failed to reload applied printer snapshot")?
            .ok_or(RepositoryError::MissingPrinter)
            .and_then(|model| printer_from_model(model, &self.access_code_cipher))?;
        transaction
            .commit()
            .await
            .context("failed to commit aggregate current-session printer snapshot")?;
        Ok(printer)
    }

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
        let nozzle_system_session_id = snapshot.nozzle_system.as_ref().map(|_| session_id);
        let presence_session_id = snapshot.telemetry_authoritative.then_some(session_id);
        let printer = upsert_snapshot_in_transaction(
            &transaction,
            tenant_id,
            agent_id,
            snapshot,
            SnapshotSessionState {
                device_features: features,
                device_features_session_id: feature_session_id,
                nozzle_system_session_id,
                mqtt_presence_session_id: presence_session_id,
            },
            &self.access_code_cipher,
        )
        .await?;
        transaction
            .commit()
            .await
            .context("failed to commit current-session printer snapshot with device features")?;
        Ok(printer)
    }

    pub async fn update_secondary_device_features_if_current(
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
        let printer = transaction
            .lock_for_update(query)
            .one(&transaction)
            .await
            .context("failed to lock printer for secondary Bambu device feature update")?;
        let Some(printer) = printer else {
            transaction
                .commit()
                .await
                .context("failed to commit missing secondary Bambu device feature update")?;
            return Ok(DeviceFeatureUpdateOutcome::StaleOrMissing);
        };

        let unchanged = match features {
            Some(features) => {
                printer.bambu_fun2_bits.as_deref() == Some(features.to_hex().as_str())
                    && printer.bambu_fun2_session_id.as_deref() == Some(session_id)
            }
            None => printer.bambu_fun2_session_id.is_none(),
        };
        if unchanged {
            transaction
                .commit()
                .await
                .context("failed to commit unchanged secondary Bambu device feature update")?;
            return Ok(DeviceFeatureUpdateOutcome::Unchanged);
        }
        let mut active = printer.into_active_model();
        match features {
            Some(features) => {
                active.bambu_fun2_bits = Set(Some(features.to_hex()));
                active.bambu_fun2_session_id = Set(Some(session_id.to_owned()));
            }
            None => active.bambu_fun2_session_id = Set(None),
        }
        active
            .update(&transaction)
            .await
            .context("failed to update secondary Bambu device features")?;
        transaction
            .commit()
            .await
            .context("failed to commit secondary Bambu device feature update")?;
        Ok(DeviceFeatureUpdateOutcome::Updated)
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
        let printer = transaction
            .lock_for_update(query)
            .one(&transaction)
            .await
            .context("failed to lock printer for Bambu device feature update")?;
        let Some(printer) = printer else {
            transaction
                .commit()
                .await
                .context("failed to commit missing Bambu device feature update")?;
            return Ok(DeviceFeatureUpdateOutcome::StaleOrMissing);
        };

        let unchanged = match features {
            Some(features) => {
                printer.bambu_fun_bits.as_deref() == Some(features.to_hex().as_str())
                    && printer.bambu_fun_session_id.as_deref() == Some(session_id)
            }
            None => printer.bambu_fun_session_id.is_none(),
        };
        if unchanged {
            transaction
                .commit()
                .await
                .context("failed to commit unchanged Bambu device feature update")?;
            return Ok(DeviceFeatureUpdateOutcome::Unchanged);
        }
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
