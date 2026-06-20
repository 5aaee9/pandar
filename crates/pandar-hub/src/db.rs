use std::{str::FromStr, time::Duration};

use anyhow::{Context, bail};
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
}
