use anyhow::Context;
use sqlx::{Executor, Postgres, Sqlite, Transaction};

use crate::{artifacts::ArtifactStorage, db::Database};

mod artifacts;
mod options;
mod plan;
mod sql;

use options::CleanupCutoffs;
pub use options::{CleanupMode, CleanupOptions, CleanupSummary};
use plan::{CleanupCategory, CleanupPlan, CleanupSelection};

pub async fn cleanup_database(
    database: &Database,
    artifact_storage: Option<&dyn ArtifactStorage>,
    options: CleanupOptions,
    mode: CleanupMode,
) -> anyhow::Result<CleanupSummary> {
    let plan = CleanupPlan::new(CleanupCutoffs::from_options(&options)?);
    let jobs = plan.selection(CleanupCategory::Jobs);
    let artifacts = plan.selection(CleanupCategory::Artifacts);
    let commands = plan.selection(CleanupCategory::Commands);
    let machine_events = plan.selection(CleanupCategory::MachineEvents);
    let audit_events = plan.selection(CleanupCategory::AuditEvents);
    let plugin_login_tickets = plan.selection(CleanupCategory::PluginLoginTickets);
    let tenant_tokens = plan.selection(CleanupCategory::TenantTokens);
    let summary = CleanupSummary {
        jobs: count(database, &jobs).await?,
        artifact_ids: strings(database, artifacts.sql, &artifacts.binds).await?,
        artifact_storage_paths: artifact_strings(database, "storage_path", &artifacts).await?,
        artifact_bytes: artifact_bytes(database, &artifacts).await?,
        artifacts: count(database, &artifacts).await?,
        commands: count(database, &commands).await?,
        machine_events: count(database, &machine_events).await?,
        audit_events: count(database, &audit_events).await?,
        plugin_login_tickets: count(database, &plugin_login_tickets).await?,
        tenant_tokens: count(database, &tenant_tokens).await?,
    };

    if mode == CleanupMode::Execute {
        execute_cleanup(database, &plan).await?;
        crate::artifacts::lifecycle::reap_expired_reservations(database).await?;
        if let Some(artifact_storage) = artifact_storage {
            crate::artifacts::lifecycle::drain_deletions(database, artifact_storage).await?;
        }
    }

    Ok(summary)
}

async fn execute_cleanup(database: &Database, plan: &CleanupPlan) -> anyhow::Result<()> {
    let jobs = plan.selection(CleanupCategory::Jobs);
    let artifacts = plan.selection(CleanupCategory::Artifacts);
    artifacts::cleanup_jobs_and_artifacts(database, &jobs, &artifacts).await?;

    for category in [
        CleanupCategory::Commands,
        CleanupCategory::MachineEvents,
        CleanupCategory::AuditEvents,
        CleanupCategory::PluginLoginTickets,
        CleanupCategory::TenantTokens,
    ] {
        let selection = plan.selection(category);
        delete_category(
            database,
            &selection.delete_sql(),
            &selection.binds,
            selection.label,
        )
        .await?;
    }
    Ok(())
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

async fn count(database: &Database, selection: &CleanupSelection<'_>) -> anyhow::Result<i64> {
    scalar(
        database,
        &format!("SELECT COUNT(*) FROM ({}) selected", selection.sql),
        &selection.binds,
    )
    .await
}

async fn artifact_bytes(
    database: &Database,
    selection: &CleanupSelection<'_>,
) -> anyhow::Result<i64> {
    scalar(
        database,
        &format!(
            "SELECT CAST(COALESCE(SUM(size_bytes), 0) AS BIGINT) FROM job_artifacts WHERE id IN ({})",
            selection.sql
        ),
        &selection.binds,
    )
    .await
}

async fn artifact_strings(
    database: &Database,
    column: &'static str,
    selection: &CleanupSelection<'_>,
) -> anyhow::Result<Vec<String>> {
    strings(
        database,
        &format!(
            "SELECT {column} FROM job_artifacts WHERE id IN ({})",
            selection.sql
        ),
        &selection.binds,
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
