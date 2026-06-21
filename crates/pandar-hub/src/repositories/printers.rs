use anyhow::Context;
use pandar_core::{AgentId, Printer, PrinterParts, TenantId};
use sqlx::{Row, postgres::PgRow, sqlite::SqliteRow};

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
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
        let count = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM printers")
                    .fetch_one(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM printers")
                    .fetch_one(pool)
                    .await
            }
        }
        .context("failed to count printers")?;

        Ok(count)
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<Printer>> {
        if !self.tenant_exists(tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = ?1
                     ORDER BY created_at ASC, id ASC",
                )
                    .bind(tenant_id.to_string())
                    .fetch_all(pool)
                    .await
                    .context("failed to list SQLite printers")?;
                rows.into_iter().map(printer_from_sqlite_row).collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = $1
                     ORDER BY created_at ASC, id ASC",
                )
                    .bind(tenant_id.to_string())
                    .fetch_all(pool)
                    .await
                    .context("failed to list PostgreSQL printers")?;
                rows.into_iter().map(printer_from_postgres_row).collect()
            }
        }
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
    ) -> RepositoryResult<Option<Printer>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id.to_string())
                .bind(printer_id)
                .fetch_optional(pool)
                .await
                .context("failed to get SQLite printer")?;
                rows.map(printer_from_sqlite_row).transpose()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = $1 AND id = $2",
                )
                .bind(tenant_id.to_string())
                .bind(printer_id)
                .fetch_optional(pool)
                .await
                .context("failed to get PostgreSQL printer")?;
                rows.map(printer_from_postgres_row).transpose()
            }
        }
    }

    pub async fn upsert_snapshot(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        snapshot: PrinterSnapshotUpsert,
    ) -> RepositoryResult<Printer> {
        if !self.agent_belongs_to_tenant(tenant_id, agent_id).await? {
            return Err(RepositoryError::MissingAgent);
        }

        let printer_id = uuid::Uuid::new_v4().to_string();
        match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                     ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                         agent_id = excluded.agent_id,
                         name = excluded.name,
                         model = excluded.model,
                         status = excluded.status,
                         last_seen_at = excluded.last_seen_at",
                )
                .bind(printer_id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(&snapshot.serial_number)
                .bind(&snapshot.name)
                .bind(&snapshot.model)
                .bind(&snapshot.status)
                .bind(&snapshot.observed_at)
                .execute(pool)
                .await
                .context("failed to upsert SQLite printer snapshot")?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
                     ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                         agent_id = excluded.agent_id,
                         name = excluded.name,
                         model = excluded.model,
                         status = excluded.status,
                         last_seen_at = excluded.last_seen_at",
                )
                .bind(printer_id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(&snapshot.serial_number)
                .bind(&snapshot.name)
                .bind(&snapshot.model)
                .bind(&snapshot.status)
                .bind(&snapshot.observed_at)
                .execute(pool)
                .await
                .context("failed to upsert PostgreSQL printer snapshot")?;
            }
        }

        self.get_by_serial_for_tenant(tenant_id, &snapshot.serial_number)
            .await?
            .ok_or_else(|| anyhow::anyhow!("printer snapshot missing after upsert").into())
    }

    async fn get_by_serial_for_tenant(
        &self,
        tenant_id: TenantId,
        serial_number: &str,
    ) -> RepositoryResult<Option<Printer>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = ?1 AND serial_number = ?2",
                )
                .bind(tenant_id.to_string())
                .bind(serial_number)
                .fetch_optional(pool)
                .await
                .context("failed to get SQLite printer by serial number")?;
                row.map(printer_from_sqlite_row).transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at
                     FROM printers
                     WHERE tenant_id = $1 AND serial_number = $2",
                )
                .bind(tenant_id.to_string())
                .bind(serial_number)
                .fetch_optional(pool)
                .await
                .context("failed to get PostgreSQL printer by serial number")?;
                row.map(printer_from_postgres_row).transpose()
            }
        }
    }

    async fn tenant_exists(&self, tenant_id: TenantId) -> RepositoryResult<bool> {
        let exists = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = ?1")
                    .bind(tenant_id.to_string())
                    .fetch_optional(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = $1")
                    .bind(tenant_id.to_string())
                    .fetch_optional(pool)
                    .await
            }
        }
        .context("failed to check tenant existence for printer repository")?;

        Ok(exists.is_some())
    }

    async fn agent_belongs_to_tenant(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<bool> {
        let exists = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT 1 FROM agents WHERE id = ?1 AND tenant_id = ?2",
                )
                .bind(agent_id.to_string())
                .bind(tenant_id.to_string())
                .fetch_optional(pool)
                .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT 1 FROM agents WHERE id = $1 AND tenant_id = $2",
                )
                .bind(agent_id.to_string())
                .bind(tenant_id.to_string())
                .fetch_optional(pool)
                .await
            }
        }
        .context("failed to check agent ownership for printer repository")?;

        Ok(exists.is_some())
    }
}

fn printer_from_sqlite_row(row: SqliteRow) -> RepositoryResult<Printer> {
    printer_from_persisted(PersistedPrinter {
        id: row.try_get("id").context("failed to read printer id")?,
        tenant_id: row
            .try_get("tenant_id")
            .context("failed to read printer tenant_id")?,
        agent_id: row
            .try_get("agent_id")
            .context("failed to read printer agent_id")?,
        serial_number: row
            .try_get("serial_number")
            .context("failed to read printer serial_number")?,
        name: row.try_get("name").context("failed to read printer name")?,
        model: row
            .try_get("model")
            .context("failed to read printer model")?,
        status: row
            .try_get("status")
            .context("failed to read printer status")?,
        last_seen_at: row
            .try_get("last_seen_at")
            .context("failed to read printer last_seen_at")?,
        created_at: row
            .try_get("created_at")
            .context("failed to read printer created_at")?,
    })
}

fn printer_from_postgres_row(row: PgRow) -> RepositoryResult<Printer> {
    printer_from_persisted(PersistedPrinter {
        id: row.try_get("id").context("failed to read printer id")?,
        tenant_id: row
            .try_get("tenant_id")
            .context("failed to read printer tenant_id")?,
        agent_id: row
            .try_get("agent_id")
            .context("failed to read printer agent_id")?,
        serial_number: row
            .try_get("serial_number")
            .context("failed to read printer serial_number")?,
        name: row.try_get("name").context("failed to read printer name")?,
        model: row
            .try_get("model")
            .context("failed to read printer model")?,
        status: row
            .try_get("status")
            .context("failed to read printer status")?,
        last_seen_at: row
            .try_get("last_seen_at")
            .context("failed to read printer last_seen_at")?,
        created_at: row
            .try_get("created_at")
            .context("failed to read printer created_at")?,
    })
}

struct PersistedPrinter {
    id: String,
    tenant_id: String,
    agent_id: String,
    serial_number: String,
    name: String,
    model: Option<String>,
    status: String,
    last_seen_at: String,
    created_at: String,
}

fn printer_from_persisted(row: PersistedPrinter) -> RepositoryResult<Printer> {
    (|| {
        Printer::from_parts(PrinterParts {
            id: row.id,
            tenant_id: TenantId::parse(&row.tenant_id).map_err(anyhow::Error::from)?,
            agent_id: AgentId::parse(&row.agent_id).map_err(anyhow::Error::from)?,
            serial_number: row.serial_number,
            name: row.name,
            model: row.model,
            status: row.status,
            last_seen_at: row.last_seen_at,
            created_at: row.created_at,
        })
        .map_err(anyhow::Error::from)
    })()
    .context("failed to rehydrate printer")
    .map_err(RepositoryError::from)
}
