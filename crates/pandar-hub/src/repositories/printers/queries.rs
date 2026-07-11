use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::{
    entities::printers,
    repositories::{RepositoryError, RepositoryResult},
};

use super::{PrinterRepository, PrinterWithLiveStatus, live_status};

impl PrinterRepository {
    pub async fn list_with_live_status_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<PrinterWithLiveStatus>> {
        let connection = self.database.sea_orm_connection();
        if !super::tenant_exists(&connection, tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        printers::Entity::find()
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(printers::Column::CreatedAt)
            .order_by_asc(printers::Column::Id)
            .all(&connection)
            .await
            .context("failed to list printers with live status")?
            .into_iter()
            .map(live_status::from_model)
            .collect()
    }

    pub async fn get_with_live_status_for_tenant(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
    ) -> RepositoryResult<Option<PrinterWithLiveStatus>> {
        printers::Entity::find_by_id(printer_id)
            .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get printer with live status")?
            .map(live_status::from_model)
            .transpose()
    }
}
