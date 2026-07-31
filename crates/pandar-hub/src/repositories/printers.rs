use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, Printer, PrinterNozzleTemperature, TenantId};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, TransactionTrait,
    sea_query::{Expr, ExprTrait},
};

#[cfg(test)]
use crate::entities::agents;
use crate::{
    db::Database,
    entities::{printers, tenants},
    printer_secrets::PrinterAccessCodeCipher,
    repositories::{
        AuditActor, RepositoryError, RepositoryResult, adapters,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
        begin_current_agent_transaction,
    },
};

mod audit_metadata;
mod device_features;
mod firmware;
mod live_status;
mod queries;
mod rows;

use crate::db::ConnectionDialectExt;
use audit_metadata::{PrinterDeleteAuditMetadata, PrinterUpdateAuditMetadata};
pub use device_features::DeviceFeatureUpdateOutcome;
pub use firmware::PrinterFirmwareUpdateOutcome;
pub use live_status::{PrinterHms, PrinterLiveStatus, PrinterWithLiveStatus};
pub(crate) use live_status::{
    PrinterLiveStatusPatch, from_model as live_status_from_model, merge_live_report,
    persist_merged_live_status,
};
use rows::printer_from_model;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterSnapshotUpsert {
    pub serial_number: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: Option<String>,
    pub observed_at: String,
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_target_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    pub connection_authoritative: bool,
    pub telemetry_authoritative: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotSessionState<'a> {
    pub(crate) device_features: Option<BambuDeviceFeatures>,
    pub(crate) device_features_session_id: Option<&'a str>,
    pub(crate) mqtt_presence_session_id: Option<&'a str>,
}

const EMPTY_SNAPSHOT_SESSION_STATE: SnapshotSessionState<'static> = SnapshotSessionState {
    device_features: None,
    device_features_session_id: None,
    mqtt_presence_session_id: None,
};

#[derive(Debug, Clone)]
pub struct PrinterRepository {
    database: Database,
    access_code_cipher: PrinterAccessCodeCipher,
}

impl PrinterRepository {
    pub(crate) fn new_with_cipher(
        database: Database,
        access_code_cipher: PrinterAccessCodeCipher,
    ) -> Self {
        Self {
            database,
            access_code_cipher,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(database: Database) -> Self {
        Self::new_with_cipher(
            database,
            crate::printer_secrets::configured_printer_access_code_cipher()
                .expect("test printer access-code cipher is valid"),
        )
    }

    #[cfg(test)]
    pub(crate) fn access_code_cipher(&self) -> PrinterAccessCodeCipher {
        self.access_code_cipher.clone()
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        let count = printers::Entity::find()
            .count(&self.database.sea_orm_connection())
            .await
            .context("failed to count printers")?;

        Ok(count.try_into().expect("printer count should fit in i64"))
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<Printer>> {
        let connection = self.database.sea_orm_connection();
        if !tenant_exists(&connection, tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        printers::Entity::find()
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(printers::Column::CreatedAt)
            .order_by_asc(printers::Column::Id)
            .all(&connection)
            .await
            .context("failed to list printers")?
            .into_iter()
            .map(|model| printer_from_model(model, &self.access_code_cipher))
            .collect()
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
    ) -> RepositoryResult<Option<Printer>> {
        printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get printer")?
            .map(|model| printer_from_model(model, &self.access_code_cipher))
            .transpose()
    }

    pub async fn delete_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<PrinterWithLiveStatus> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin printer delete audit transaction")?;
        let query = printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()));
        let model = tx
            .lock_for_update(query)
            .one(&tx)
            .await
            .context("failed to lock printer before delete")?;
        let Some(model) = model else {
            return Err(RepositoryError::MissingPrinter);
        };

        let printer = live_status_from_model(model, &self.access_code_cipher)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "printer.delete",
                "printer",
                Some(printer.printer.id.clone()),
                audit_metadata(PrinterDeleteAuditMetadata {
                    printer_name: &printer.printer.name,
                    serial_number: &printer.printer.serial_number,
                    agent_id: printer.printer.agent_id.to_string(),
                    previous_status: &printer.printer.status,
                }),
            ),
        )
        .await?;
        printers::Entity::delete_by_id(printer_id)
            .exec(&tx)
            .await
            .context("failed to delete printer")?;
        tx.commit()
            .await
            .context("failed to commit printer delete audit transaction")?;

        Ok(printer)
    }

    pub async fn update_details_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        name: String,
        host: String,
        access_code: String,
        actor: AuditActor,
    ) -> RepositoryResult<Printer> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin printer update audit transaction")?;
        let Some(model) = printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .one(&tx)
            .await
            .context("failed to get printer before update")?
        else {
            return Err(RepositoryError::MissingPrinter);
        };

        let previous_name = model.name.clone();
        let previous_host = model.host.clone();
        printers::Entity::update_many()
            .filter(printers::Column::Id.eq(printer_id))
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .set(printers::ActiveModel {
                name: Set(name),
                host: Set(Some(host)),
                access_code: Set(None),
                access_code_encrypted: Set(Some(self.access_code_cipher.encrypt(
                    &tenant_id.to_string(),
                    &model.serial_number,
                    &access_code,
                )?)),
                ..Default::default()
            })
            .col_expr(
                printers::Column::StateRevision,
                Expr::col(printers::Column::StateRevision).add(1),
            )
            .exec(&tx)
            .await
            .context("failed to update printer details")?;
        let model = printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .one(&tx)
            .await
            .context("failed to reload printer details")?
            .ok_or(RepositoryError::MissingPrinter)?;
        let printer = printer_from_model(model, &self.access_code_cipher)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "printer.update",
                "printer",
                Some(printer.id.clone()),
                audit_metadata(PrinterUpdateAuditMetadata {
                    previous_name: &previous_name,
                    previous_host: &previous_host,
                    printer_name: &printer.name,
                    printer_host: &printer.host,
                    serial_number: &printer.serial_number,
                    agent_id: printer.agent_id.to_string(),
                }),
            ),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit printer update audit transaction")?;

        Ok(printer)
    }

    #[cfg(test)]
    pub(crate) async fn upsert_snapshot(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        snapshot: PrinterSnapshotUpsert,
    ) -> RepositoryResult<Printer> {
        let connection = self.database.sea_orm_connection();
        if !agent_belongs_to_tenant(&connection, tenant_id, agent_id).await? {
            return Err(RepositoryError::MissingAgent);
        }
        let tx = self
            .database
            .begin_write_transaction()
            .await
            .context("failed to begin printer snapshot transaction")?;
        let printer = upsert_snapshot_in_transaction(
            &tx,
            tenant_id,
            agent_id,
            snapshot,
            EMPTY_SNAPSHOT_SESSION_STATE,
            &self.access_code_cipher,
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit printer snapshot transaction")?;
        Ok(printer)
    }

    pub async fn upsert_snapshot_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        snapshot: PrinterSnapshotUpsert,
    ) -> RepositoryResult<Printer> {
        let tx = begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
            .await?;
        let printer = upsert_snapshot_in_transaction(
            &tx,
            tenant_id,
            agent_id,
            snapshot,
            EMPTY_SNAPSHOT_SESSION_STATE,
            &self.access_code_cipher,
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit current-session printer snapshot transaction")?;
        Ok(printer)
    }
}

async fn upsert_snapshot_in_transaction(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshotUpsert,
    session_state: SnapshotSessionState<'_>,
    access_code_cipher: &PrinterAccessCodeCipher,
) -> RepositoryResult<Printer> {
    let access_code_encrypted = snapshot
        .access_code
        .as_deref()
        .map(|access_code| {
            access_code_cipher.encrypt(&tenant_id.to_string(), &snapshot.serial_number, access_code)
        })
        .transpose()?;
    adapters::printers::upsert_snapshot(
        transaction,
        tenant_id,
        agent_id,
        &snapshot,
        access_code_encrypted.as_deref(),
        session_state,
    )
    .await?;
    printers::Entity::find()
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(&snapshot.serial_number))
        .one(transaction)
        .await
        .context("failed to reload printer snapshot")?
        .ok_or_else(|| anyhow::anyhow!("printer snapshot missing after upsert").into())
        .and_then(|model| printer_from_model(model, access_code_cipher))
}

async fn tenant_exists<C>(connection: &C, tenant_id: TenantId) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    tenants::Entity::find_by_id(tenant_id.to_string())
        .one(connection)
        .await
        .context("failed to check tenant existence for printer repository")
        .map(|tenant| tenant.is_some())
        .map_err(Into::into)
}

#[cfg(test)]
async fn agent_belongs_to_tenant<C>(
    connection: &C,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    agents::Entity::find_by_id(agent_id.to_string())
        .filter(agents::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to check agent ownership for printer repository")
        .map(|agent| agent.is_some())
        .map_err(Into::into)
}
