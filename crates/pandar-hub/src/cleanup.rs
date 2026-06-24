use anyhow::Context;
use sqlx::{Executor, Postgres, Sqlite, Transaction};

use crate::{artifacts::ArtifactStorage, db::Database};

mod options;
mod sql;

use options::CleanupCutoffs;
pub use options::{CleanupMode, CleanupOptions, CleanupSummary};
use sql::*;

pub async fn cleanup_database(
    database: &Database,
    artifact_storage: Option<&dyn ArtifactStorage>,
    options: CleanupOptions,
    mode: CleanupMode,
) -> anyhow::Result<CleanupSummary> {
    let cutoffs = CleanupCutoffs::from_options(&options)?;
    let summary = CleanupSummary {
        jobs: count(database, JOB_SELECTION_SQL, &[&cutoffs.jobs]).await?,
        artifact_ids: strings(
            database,
            ARTIFACT_SELECTION_SQL,
            &[&cutoffs.jobs, &cutoffs.jobs],
        )
        .await?,
        artifact_storage_paths: artifact_strings(
            database,
            "storage_path",
            &[&cutoffs.jobs, &cutoffs.jobs],
        )
        .await?,
        artifact_bytes: artifact_bytes(database, &[&cutoffs.jobs, &cutoffs.jobs]).await?,
        artifacts: artifact_count(database, &[&cutoffs.jobs, &cutoffs.jobs]).await?,
        commands: count(
            database,
            COMMAND_SELECTION_SQL,
            &[&cutoffs.commands, &cutoffs.commands],
        )
        .await?,
        machine_events: count(
            database,
            "SELECT id FROM machine_events WHERE created_at < ?",
            &[&cutoffs.machine_events],
        )
        .await?,
        audit_events: count(
            database,
            AUDIT_SELECTION_SQL,
            &[
                &cutoffs.audit,
                &cutoffs.jobs,
                &cutoffs.commands,
                &cutoffs.jobs,
            ],
        )
        .await?,
        plugin_login_tickets: count(
            database,
            PLUGIN_TICKET_SELECTION_SQL,
            &[
                &cutoffs.plugin_tickets,
                &cutoffs.plugin_tickets,
                &cutoffs.plugin_tickets,
            ],
        )
        .await?,
        tenant_tokens: count(
            database,
            TENANT_TOKEN_SELECTION_SQL,
            &[&cutoffs.tenant_tokens, &cutoffs.tenant_tokens],
        )
        .await?,
    };

    if mode == CleanupMode::Execute {
        if let Some(artifact_storage) = artifact_storage {
            delete_artifacts(artifact_storage, &summary.artifact_storage_paths).await?;
        }
        execute_cleanup(database, &cutoffs).await?;
        if artifact_storage.is_some() {
            cleanup_artifact_rows(database, &summary.artifact_ids).await?;
        }
    }

    Ok(summary)
}

pub async fn cleanup_artifact_rows(
    database: &Database,
    artifact_ids: &[String],
) -> anyhow::Result<()> {
    delete_ids(database, DELETE_ARTIFACTS_SQL, artifact_ids, "artifact").await
}

async fn execute_cleanup(database: &Database, cutoffs: &CleanupCutoffs) -> anyhow::Result<()> {
    delete_category(database, DELETE_JOBS_SQL, &[&cutoffs.jobs], "job").await?;

    delete_category(
        database,
        DELETE_COMMANDS_SQL,
        &[&cutoffs.commands, &cutoffs.commands],
        "command",
    )
    .await?;
    delete_category(
        database,
        "DELETE FROM machine_events WHERE created_at < ?",
        &[&cutoffs.machine_events],
        "machine event",
    )
    .await?;
    delete_category(
        database,
        &delete_from_selection(DELETE_AUDIT_SQL, AUDIT_SELECTION_SQL),
        &[
            &cutoffs.audit,
            &cutoffs.jobs,
            &cutoffs.commands,
            &cutoffs.jobs,
        ],
        "audit event",
    )
    .await?;
    delete_category(
        database,
        DELETE_PLUGIN_TICKETS_SQL,
        &[
            &cutoffs.plugin_tickets,
            &cutoffs.plugin_tickets,
            &cutoffs.plugin_tickets,
        ],
        "plugin login ticket",
    )
    .await?;
    delete_category(
        database,
        DELETE_TENANT_TOKENS_SQL,
        &[&cutoffs.tenant_tokens, &cutoffs.tenant_tokens],
        "tenant token",
    )
    .await
}

async fn delete_artifacts(
    artifact_storage: &dyn ArtifactStorage,
    storage_paths: &[String],
) -> anyhow::Result<()> {
    for storage_path in storage_paths {
        artifact_storage
            .delete_artifact(storage_path)
            .await
            .context("failed to delete cleanup artifact [redacted]")
            .map_err(|err| {
                anyhow::anyhow!("{}", crate::redaction::redact_secrets(&format!("{err:#}")))
            })?;
    }
    Ok(())
}

async fn delete_ids(
    database: &Database,
    sql_prefix: &'static str,
    ids: &[String],
    label: &'static str,
) -> anyhow::Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!("{sql_prefix}{placeholders})");
    let binds = ids.iter().map(String::as_str).collect::<Vec<_>>();
    delete_category(database, &sql, &binds, label).await
}

async fn delete_category(
    database: &Database,
    sql: &str,
    binds: &[&str],
    label: &'static str,
) -> anyhow::Result<()> {
    match database {
        Database::Sqlite(pool) => {
            let mut tx = pool
                .begin()
                .await
                .with_context(|| format!("failed to begin {label} cleanup transaction"))?;
            execute_sqlite(&mut tx, sql, binds).await?;
            tx.commit()
                .await
                .with_context(|| format!("failed to commit {label} cleanup transaction"))
        }
        Database::Postgres(pool) => {
            let mut tx = pool
                .begin()
                .await
                .with_context(|| format!("failed to begin {label} cleanup transaction"))?;
            execute_postgres(&mut tx, sql, binds).await?;
            tx.commit()
                .await
                .with_context(|| format!("failed to commit {label} cleanup transaction"))
        }
    }
}

async fn count(database: &Database, selection_sql: &str, binds: &[&str]) -> anyhow::Result<i64> {
    scalar(
        database,
        &format!("SELECT COUNT(*) FROM ({selection_sql}) selected"),
        binds,
    )
    .await
}

async fn artifact_count(database: &Database, binds: &[&str]) -> anyhow::Result<i64> {
    count(database, ARTIFACT_SELECTION_SQL, binds).await
}

async fn artifact_bytes(database: &Database, binds: &[&str]) -> anyhow::Result<i64> {
    scalar(
        database,
        &format!(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM job_artifacts WHERE id IN ({ARTIFACT_SELECTION_SQL})"
        ),
        binds,
    )
    .await
}

async fn artifact_strings(
    database: &Database,
    column: &'static str,
    binds: &[&str],
) -> anyhow::Result<Vec<String>> {
    strings(
        database,
        &format!("SELECT {column} FROM job_artifacts WHERE id IN ({ARTIFACT_SELECTION_SQL})"),
        binds,
    )
    .await
}

async fn scalar(
    database: &Database,
    sql: impl Into<String>,
    binds: &[&str],
) -> anyhow::Result<i64> {
    let sql = sql.into();
    match database {
        Database::Sqlite(pool) => {
            let mut statement = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
            for bind in binds {
                statement = statement.bind(*bind);
            }
            statement.fetch_one(pool).await.map_err(anyhow::Error::from)
        }
        Database::Postgres(pool) => {
            let mut statement =
                sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(postgres_sql(&sql)));
            for bind in binds {
                statement = statement.bind(*bind);
            }
            statement.fetch_one(pool).await.map_err(anyhow::Error::from)
        }
    }
}

async fn strings(database: &Database, sql: &str, binds: &[&str]) -> anyhow::Result<Vec<String>> {
    match database {
        Database::Sqlite(pool) => {
            let mut statement =
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.to_owned()));
            for bind in binds {
                statement = statement.bind(*bind);
            }
            statement.fetch_all(pool).await.map_err(anyhow::Error::from)
        }
        Database::Postgres(pool) => {
            let postgres_sql = postgres_sql(sql);
            let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(postgres_sql));
            for bind in binds {
                statement = statement.bind(*bind);
            }
            statement.fetch_all(pool).await.map_err(anyhow::Error::from)
        }
    }
}

async fn execute_sqlite(
    tx: &mut Transaction<'_, Sqlite>,
    sql: &str,
    binds: &[&str],
) -> anyhow::Result<()> {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
    for bind in binds {
        query = query.bind(*bind);
    }
    tx.execute(query)
        .await
        .context("failed to execute SQLite cleanup statement")?;
    Ok(())
}

async fn execute_postgres(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    binds: &[&str],
) -> anyhow::Result<()> {
    let postgres_sql = postgres_sql(sql);
    let mut query = sqlx::query(sqlx::AssertSqlSafe(postgres_sql));
    for bind in binds {
        query = query.bind(*bind);
    }
    tx.execute(query)
        .await
        .context("failed to execute PostgreSQL cleanup statement")?;
    Ok(())
}

fn postgres_sql(sql: &str) -> String {
    let mut next = 1;
    sql.chars()
        .map(|ch| {
            if ch == '?' {
                let placeholder = format!("${next}");
                next += 1;
                placeholder
            } else {
                ch.to_string()
            }
        })
        .collect()
}

fn delete_from_selection(prefix: &str, selection_sql: &str) -> String {
    format!("{prefix}{selection_sql})")
}
