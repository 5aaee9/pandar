use anyhow::Context;
use pandar_core::{AgentId, Printer, PrinterNozzleTemperature, PrinterParts, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, TransactionTrait,
};

use crate::{
    db::Database,
    entities::{agents, printers, tenants},
    repositories::{
        AuditActor, RepositoryError, RepositoryResult, adapters,
        audit::{insert_audit_event_tx, record_audit_event},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterSnapshotUpsert {
    pub serial_number: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub observed_at: String,
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PrinterRepository {
    database: Database,
}

impl PrinterRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
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
            .map(printer_from_model)
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
            .map(printer_from_model)
            .transpose()
    }

    pub async fn delete_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<Printer> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin printer delete audit transaction")?;
        let Some(model) = printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .one(&tx)
            .await
            .context("failed to get printer before delete")?
        else {
            return Err(RepositoryError::MissingPrinter);
        };

        let printer = printer_from_model(model)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "printer.delete",
                "printer",
                Some(printer.id.clone()),
                serde_json::json!({
                    "printer_name": printer.name.clone(),
                    "serial_number": printer.serial_number.clone(),
                    "agent_id": printer.agent_id.to_string(),
                    "previous_status": printer.status.clone(),
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

    pub async fn update_name_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        name: String,
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
        let mut active = model.into_active_model();
        active.name = Set(name);
        let model = active
            .update(&tx)
            .await
            .context("failed to update printer name")?;
        let printer = printer_from_model(model)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "printer.update",
                "printer",
                Some(printer.id.clone()),
                serde_json::json!({
                    "previous_name": previous_name,
                    "printer_name": printer.name.clone(),
                    "serial_number": printer.serial_number.clone(),
                    "agent_id": printer.agent_id.to_string(),
                }),
            ),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit printer update audit transaction")?;

        Ok(printer)
    }

    pub async fn upsert_snapshot(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        snapshot: PrinterSnapshotUpsert,
    ) -> RepositoryResult<Printer> {
        let connection = self.database.sea_orm_connection();
        if !agent_belongs_to_tenant(&connection, tenant_id, agent_id).await? {
            return Err(RepositoryError::MissingAgent);
        }

        let serial_number = snapshot.serial_number.clone();
        adapters::printers::upsert_snapshot(
            &self.database,
            tenant_id,
            agent_id,
            &uuid::Uuid::new_v4().to_string(),
            &snapshot,
        )
        .await?;

        self.get_by_serial_for_tenant(tenant_id, &serial_number)
            .await?
            .ok_or_else(|| anyhow::anyhow!("printer snapshot missing after upsert").into())
    }

    async fn get_by_serial_for_tenant(
        &self,
        tenant_id: TenantId,
        serial_number: &str,
    ) -> RepositoryResult<Option<Printer>> {
        printers::Entity::find()
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printers::Column::SerialNumber.eq(serial_number))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get printer by serial number")?
            .map(printer_from_model)
            .transpose()
    }
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

fn printer_from_model(model: printers::Model) -> RepositoryResult<Printer> {
    (|| {
        Printer::from_parts(PrinterParts {
            id: model.id,
            tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
            agent_id: AgentId::parse(&model.agent_id).map_err(anyhow::Error::from)?,
            serial_number: model.serial_number,
            host: model.host,
            access_code: model.access_code,
            name: model.name,
            model: model.model,
            status: model.status,
            last_seen_at: model
                .last_seen_at
                .context("failed to read printer last_seen_at")?,
            created_at: model.created_at,
            nozzle_temperatures: serde_json::from_str(&model.nozzle_temperatures_json)
                .context("failed to read printer nozzle temperatures")?,
            active_nozzle: model.active_nozzle,
            bed_temperature_celsius: model.bed_temperature_celsius,
            bed_target_temperature_celsius: model.bed_target_temperature_celsius,
            chamber_temperature_celsius: model.chamber_temperature_celsius,
            chamber_light_on: model.chamber_light_on,
        })
        .map_err(anyhow::Error::from)
    })()
    .context("failed to rehydrate printer")
    .map_err(RepositoryError::from)
}
