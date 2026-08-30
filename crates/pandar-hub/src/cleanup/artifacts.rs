use anyhow::Context;
use sea_orm::{ConnectionTrait, Statement};

use crate::db::Database;

use super::{
    CleanupSelection, postgres_sql, sql::DELETE_ARTIFACT_CANDIDATES_SQL,
    sql::DROP_ARTIFACT_CANDIDATES_SQL,
};

pub(super) async fn cleanup_jobs_and_artifacts(
    database: &Database,
    jobs: &CleanupSelection<'_>,
    artifacts: &CleanupSelection<'_>,
) -> anyhow::Result<()> {
    let tx = database
        .begin_write_transaction()
        .await
        .context("failed to begin job and artifact cleanup transaction")?;
    execute(&tx, DROP_ARTIFACT_CANDIDATES_SQL, &[]).await?;
    let candidate_sql = format!(
        "CREATE TEMPORARY TABLE cleanup_artifact_candidates AS {}",
        artifacts.sql
    );
    execute(&tx, &candidate_sql, &artifacts.binds).await?;
    execute(&tx, &jobs.delete_sql(), &jobs.binds).await?;
    let deleted_paths =
        query_strings(&tx, DELETE_ARTIFACT_CANDIDATES_SQL, &[], "storage_path").await?;
    for storage_path in deleted_paths {
        crate::artifacts::lifecycle::enqueue_deletion(&tx, &storage_path).await?;
    }
    execute(&tx, DROP_ARTIFACT_CANDIDATES_SQL, &[]).await?;
    tx.commit()
        .await
        .context("failed to commit job and artifact cleanup transaction")
}

async fn execute(
    tx: &sea_orm::DatabaseTransaction,
    sql: &str,
    binds: &[&str],
) -> anyhow::Result<()> {
    tx.execute_raw(Statement::from_sql_and_values(
        tx.get_database_backend(),
        sea_sql(tx, sql),
        binds.iter().map(|value| (*value).into()),
    ))
    .await
    .context("failed to execute job and artifact cleanup statement")?;
    Ok(())
}

async fn query_strings(
    tx: &sea_orm::DatabaseTransaction,
    sql: &str,
    binds: &[&str],
    column: &str,
) -> anyhow::Result<Vec<String>> {
    tx.query_all_raw(Statement::from_sql_and_values(
        tx.get_database_backend(),
        sea_sql(tx, sql),
        binds.iter().map(|value| (*value).into()),
    ))
    .await
    .context("failed to select cleanup artifacts")?
    .into_iter()
    .map(|row| row.try_get("", column).map_err(anyhow::Error::from))
    .collect()
}

fn sea_sql(tx: &sea_orm::DatabaseTransaction, sql: &str) -> String {
    if tx.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
        postgres_sql(sql)
    } else {
        sql.to_owned()
    }
}
