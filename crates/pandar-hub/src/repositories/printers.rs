use anyhow::Context;
use pandar_core::{AgentId, Printer, PrinterParts, TenantId};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

use crate::{
    db::Database,
    entities::{agents, printers, tenants},
    repositories::{RepositoryError, RepositoryResult, adapters},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterSnapshotUpsert {
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub observed_at: String,
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
            name: model.name,
            model: model.model,
            status: model.status,
            last_seen_at: model
                .last_seen_at
                .context("failed to read printer last_seen_at")?,
            created_at: model.created_at,
        })
        .map_err(anyhow::Error::from)
    })()
    .context("failed to rehydrate printer")
    .map_err(RepositoryError::from)
}
