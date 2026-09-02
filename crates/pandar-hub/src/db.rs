use std::{str::FromStr, time::Duration};

use anyhow::{Context, bail};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, QuerySelect,
    Select, SqliteTransactionMode, SqlxPostgresConnector, SqlxSqliteConnector, TransactionOptions,
    TransactionTrait,
};
use sqlx::{
    PgPool, SqlitePool,
    migrate::Migrator,
    postgres::PgPoolOptions,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("migrations/sqlite");
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("migrations/postgres");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    url: String,
    backend: DatabaseBackend,
}

impl DatabaseConfig {
    pub fn from_url(url: impl Into<String>) -> anyhow::Result<Self> {
        let url = url.into();
        let backend = if url.starts_with("sqlite:") {
            DatabaseBackend::Sqlite
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            DatabaseBackend::Postgres
        } else {
            bail!("unsupported database URL scheme");
        };

        Ok(Self { url, backend })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone)]
pub enum Database {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> anyhow::Result<Self> {
        match config.backend {
            DatabaseBackend::Sqlite => {
                let options = SqliteConnectOptions::from_str(config.url())
                    .with_context(|| format!("failed to parse SQLite URL {}", config.url()))?
                    .create_if_missing(true)
                    .foreign_keys(true)
                    .journal_mode(SqliteJournalMode::Wal);
                let max_connections = if config.url() == "sqlite::memory:" {
                    1
                } else {
                    5
                };
                let pool = SqlitePoolOptions::new()
                    .max_connections(max_connections)
                    .acquire_timeout(Duration::from_secs(5))
                    .connect_with(options)
                    .await
                    .with_context(|| {
                        format!("failed to connect to SQLite database {}", config.url())
                    })?;

                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&pool)
                    .await
                    .context("failed to enable SQLite foreign keys")?;

                Ok(Self::Sqlite(pool))
            }
            DatabaseBackend::Postgres => {
                let pool = PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(Duration::from_secs(5))
                    .connect(config.url())
                    .await
                    .with_context(|| {
                        format!("failed to connect to PostgreSQL database {}", config.url())
                    })?;

                Ok(Self::Postgres(pool))
            }
        }
    }

    pub fn backend(&self) -> DatabaseBackend {
        match self {
            Self::Sqlite(_) => DatabaseBackend::Sqlite,
            Self::Postgres(_) => DatabaseBackend::Postgres,
        }
    }

    pub fn sea_orm_connection(&self) -> DatabaseConnection {
        match self {
            Self::Sqlite(pool) => SqlxSqliteConnector::from_sqlx_sqlite_pool(pool.clone()),
            Self::Postgres(pool) => SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone()),
        }
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        match self {
            Self::Sqlite(pool) => {
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(pool)
                    .await
                    .context("failed to enable SQLite foreign keys before migrations")?;
                SQLITE_MIGRATOR
                    .run(pool)
                    .await
                    .context("failed to run SQLite migrations")?;
            }
            Self::Postgres(pool) => {
                POSTGRES_MIGRATOR
                    .run(pool)
                    .await
                    .context("failed to run PostgreSQL migrations")?;
            }
        }

        Ok(())
    }

    /// Begin a transaction suitable for read-modify-write flows: immediate on
    /// SQLite so the write lock is taken up front, plain on PostgreSQL.
    pub async fn begin_write_transaction(&self) -> Result<DatabaseTransaction, DbErr> {
        self.sea_orm_connection().begin_write_transaction().await
    }

    /// Begin a read-only snapshot transaction: repeatable-read on PostgreSQL
    /// so paginated reads see one consistent view, plain elsewhere.
    pub async fn begin_snapshot_transaction(&self) -> Result<DatabaseTransaction, DbErr> {
        self.sea_orm_connection().begin_snapshot_transaction().await
    }
}

/// A unique constraint the hub relies on at the repository layer. Each variant
/// privately owns the constraint spellings both backends embed in error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqueConstraint {
    TenantSlug,
    AgentName,
    TenantTokenHash,
    PluginLoginTicketHash,
    JoinLinkTokenHash,
    UserEmail,
    UserIdentityExternal,
    UserIdentityUserProvider,
    JobFilamentUsageSlot,
    PersonalPresetName,
    ArtifactDeletionStoragePath,
}

impl UniqueConstraint {
    fn spellings(self) -> (&'static str, &'static str) {
        match self {
            Self::TenantSlug => ("tenants.slug", "tenants_slug_key"),
            Self::AgentName => ("agents.tenant_id, agents.name", "agents_tenant_id_name_key"),
            Self::TenantTokenHash => ("tenant_tokens.token_hash", "tenant_tokens_token_hash_key"),
            Self::PluginLoginTicketHash => (
                "plugin_login_tickets.ticket_hash",
                "plugin_login_tickets_ticket_hash_key",
            ),
            Self::JoinLinkTokenHash => ("join_links.token_hash", "join_links_token_hash_key"),
            Self::UserEmail => ("users.tenant_id, users.email", "users_tenant_id_email_key"),
            Self::UserIdentityExternal => (
                "user_identities.tenant_id, user_identities.provider, user_identities.subject",
                "user_identities_tenant_id_provider_subject_key",
            ),
            Self::UserIdentityUserProvider => (
                "user_identities.tenant_id, user_identities.user_id, user_identities.provider",
                "user_identities_tenant_id_user_id_provider_key",
            ),
            Self::JobFilamentUsageSlot => (
                "job_filament_usages.tenant_id, job_filament_usages.job_id, job_filament_usages.slot_index, job_filament_usages.source",
                "job_filament_usages_tenant_id_job_id_slot_index_source_key",
            ),
            Self::PersonalPresetName => (
                "personal_presets.tenant_id, personal_presets.owner_user_id, personal_presets.name",
                "personal_presets_tenant_id_owner_user_id_name_key",
            ),
            Self::ArtifactDeletionStoragePath => (
                "artifact_deletions.storage_path",
                "artifact_deletions_storage_path_key",
            ),
        }
    }
}

pub(crate) fn is_unique_violation(err: &DbErr, constraint: UniqueConstraint) -> bool {
    let (sqlite_spelling, postgres_spelling) = constraint.spellings();
    if let Some(sea_orm::SqlErr::UniqueConstraintViolation(message)) = err.sql_err()
        && (message.contains(sqlite_spelling) || message.contains(postgres_spelling))
    {
        return true;
    }

    let message = err.to_string();
    message.contains(sqlite_spelling) || message.contains(postgres_spelling)
}

pub(crate) fn is_foreign_key_violation(err: &DbErr) -> bool {
    if matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::ForeignKeyConstraintViolation(_))
    ) {
        return true;
    }

    let message = err.to_string();
    message.contains("23503") || message.contains("FOREIGN KEY constraint failed")
}

/// Dialect behaviour available to code holding any sea-orm connection or
/// transaction, without access to the originating [`Database`] handle.
pub(crate) trait ConnectionDialectExt: ConnectionTrait {
    /// Lock the selected rows for update on PostgreSQL; a no-op elsewhere,
    /// where the write transaction already serializes access.
    fn lock_for_update<E: EntityTrait>(&self, select: Select<E>) -> Select<E> {
        match self.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => select.lock_exclusive(),
            _ => select,
        }
    }

    /// Take a SHARE table lock on PostgreSQL; a no-op elsewhere, where the
    /// write transaction already serializes access. Table names are internal
    /// constants, never external input.
    async fn lock_tables_in_share_mode(&self, tables: &[&str]) -> Result<(), DbErr> {
        if self.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            self.execute_unprepared(&format!("LOCK TABLE {} IN SHARE MODE", tables.join(", ")))
                .await?;
        }
        Ok(())
    }
}

impl<T: ConnectionTrait> ConnectionDialectExt for T {}

/// Dialect-aware transaction starts for code holding a sea-orm connection.
pub(crate) trait TransactionDialectExt:
    TransactionTrait<Transaction = DatabaseTransaction> + ConnectionTrait
{
    async fn begin_write_transaction(&self) -> Result<DatabaseTransaction, DbErr> {
        self.begin_with_options(TransactionOptions {
            sqlite_transaction_mode: matches!(
                self.get_database_backend(),
                sea_orm::DatabaseBackend::Sqlite
            )
            .then_some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
    }

    /// Begin a read-only snapshot transaction: repeatable-read on PostgreSQL
    /// so paginated reads see one consistent view, plain elsewhere.
    async fn begin_snapshot_transaction(&self) -> Result<DatabaseTransaction, DbErr> {
        self.begin_with_options(TransactionOptions {
            isolation_level: matches!(
                self.get_database_backend(),
                sea_orm::DatabaseBackend::Postgres
            )
            .then_some(sea_orm::IsolationLevel::RepeatableRead),
            ..Default::default()
        })
        .await
    }
}

impl<T: TransactionTrait<Transaction = DatabaseTransaction> + ConnectionTrait> TransactionDialectExt
    for T
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_config_detects_sqlite_backend() {
        let config = DatabaseConfig::from_url("sqlite::memory:").unwrap();

        assert_eq!(config.backend(), DatabaseBackend::Sqlite);
        assert_eq!(config.url(), "sqlite::memory:");
    }

    #[test]
    fn database_config_detects_postgres_backend() {
        let config = DatabaseConfig::from_url("postgres://localhost/pandar").unwrap();

        assert_eq!(config.backend(), DatabaseBackend::Postgres);
    }

    #[test]
    fn database_config_rejects_unsupported_scheme() {
        assert!(DatabaseConfig::from_url("mysql://localhost/pandar").is_err());
    }

    #[test]
    fn unique_violation_matches_sqlite_spelling() {
        let err = DbErr::Custom("UNIQUE constraint failed: tenants.slug".to_owned());

        assert!(is_unique_violation(&err, UniqueConstraint::TenantSlug));
        assert!(!is_unique_violation(&err, UniqueConstraint::UserEmail));
    }

    #[test]
    fn unique_violation_matches_postgres_spelling() {
        let err = DbErr::Custom(
            "duplicate key value violates unique constraint \"tenants_slug_key\"".to_owned(),
        );

        assert!(is_unique_violation(&err, UniqueConstraint::TenantSlug));
        assert!(!is_unique_violation(&err, UniqueConstraint::AgentName));
    }

    #[test]
    fn every_constraint_has_distinct_spellings() {
        let constraints = [
            UniqueConstraint::TenantSlug,
            UniqueConstraint::AgentName,
            UniqueConstraint::TenantTokenHash,
            UniqueConstraint::PluginLoginTicketHash,
            UniqueConstraint::JoinLinkTokenHash,
            UniqueConstraint::UserEmail,
            UniqueConstraint::UserIdentityExternal,
            UniqueConstraint::UserIdentityUserProvider,
            UniqueConstraint::JobFilamentUsageSlot,
            UniqueConstraint::PersonalPresetName,
            UniqueConstraint::ArtifactDeletionStoragePath,
        ];
        let spellings: Vec<_> = constraints.iter().map(|c| c.spellings()).collect();
        for (index, spelling) in spellings.iter().enumerate() {
            assert!(!spellings[..index].contains(spelling));
        }
    }
}
