use anyhow::Context;

use crate::{db::Database, repositories::RepositoryResult};

#[derive(Debug, Clone)]
pub struct PrinterRepository {
    database: Database,
}

impl PrinterRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        count_table(&self.database, "printers", "failed to count printers").await
    }
}

async fn count_table(
    database: &Database,
    table: &str,
    context: &'static str,
) -> RepositoryResult<i64> {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    let count = match database {
        Database::Sqlite(pool) => sqlx::query_scalar(&sql).fetch_one(pool).await,
        Database::Postgres(pool) => sqlx::query_scalar(&sql).fetch_one(pool).await,
    }
    .context(context)?;

    Ok(count)
}
