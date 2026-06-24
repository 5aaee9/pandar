use anyhow::Context;
use sea_orm::{ConnectionTrait, Statement};

use crate::{
    AppState,
    db::{Database, DatabaseBackend},
};

pub async fn prometheus_metrics(state: &AppState) -> anyhow::Result<String> {
    crate::readiness::check(state).await;
    let mut output = String::new();
    append_agent_sessions(&mut output, state).await?;
    append_commands(&mut output, state.database()).await?;
    append_websockets(&mut output, state).await;
    append_control_plane(&mut output, state);
    append_jobs(&mut output, state.database()).await?;
    append_print_reports(&mut output, state);
    append_readyz(&mut output, state).await;
    Ok(output)
}

async fn append_agent_sessions(output: &mut String, state: &AppState) -> anyhow::Result<()> {
    let online = state.sessions().count().await;
    let total = state.agents().count().await?;
    output.push_str(&format!(
        "pandar_agent_sessions{{state=\"online\"}} {online}\n"
    ));
    output.push_str(&format!(
        "pandar_agent_sessions{{state=\"offline\"}} {}\n",
        (total - online).max(0)
    ));
    Ok(())
}

async fn append_commands(output: &mut String, database: &Database) -> anyhow::Result<()> {
    let rows = grouped_counts(database, "commands", &["kind", "status"]).await?;
    if rows.is_empty() {
        output.push_str("pandar_commands_total{kind=\"none\",status=\"none\"} 0\n");
    }
    for row in rows {
        output.push_str(&format!(
            "pandar_commands_total{{kind=\"{}\",status=\"{}\"}} {}\n",
            escape_label(&row.labels[0]),
            escape_label(&row.labels[1]),
            row.count
        ));
    }
    Ok(())
}

async fn append_websockets(output: &mut String, state: &AppState) {
    let subscriptions = state.metrics().websocket_subscription_snapshot().await;
    if subscriptions.is_empty() {
        output.push_str("pandar_websocket_subscriptions{tenant_id_hash=\"none\"} 0\n");
    }
    for (tenant_id_hash, count) in subscriptions {
        output.push_str(&format!(
            "pandar_websocket_subscriptions{{tenant_id_hash=\"{}\"}} {count}\n",
            escape_label(&tenant_id_hash)
        ));
    }
    for (result, count) in state.metrics().websocket_ticket_snapshot() {
        output.push_str(&format!(
            "pandar_websocket_tickets_total{{result=\"{result}\"}} {count}\n"
        ));
    }
}

fn append_control_plane(output: &mut String, state: &AppState) {
    for (result, count) in state.metrics().control_plane_snapshot() {
        output.push_str(&format!(
            "pandar_control_plane_messages_total{{result=\"{result}\"}} {count}\n"
        ));
    }
}

async fn append_jobs(output: &mut String, database: &Database) -> anyhow::Result<()> {
    let rows = grouped_counts(database, "jobs", &["status", "print_status"]).await?;
    if rows.is_empty() {
        output.push_str("pandar_jobs_total{status=\"none\",print_status=\"none\"} 0\n");
    }
    for row in rows {
        output.push_str(&format!(
            "pandar_jobs_total{{status=\"{}\",print_status=\"{}\"}} {}\n",
            escape_label(&row.labels[0]),
            escape_label(&row.labels[1]),
            row.count
        ));
    }
    Ok(())
}

fn append_print_reports(output: &mut String, state: &AppState) {
    for (result, count) in state.metrics().print_report_snapshot() {
        output.push_str(&format!(
            "pandar_print_reports_total{{result=\"{result}\"}} {count}\n"
        ));
    }
}

async fn append_readyz(output: &mut String, state: &AppState) {
    let readyz = state.metrics().readyz_snapshot().await;
    if readyz.is_empty() {
        for check in ["database", "grpc", "artifact_storage", "external_auth"] {
            output.push_str(&format!("pandar_readyz{{check=\"{check}\"}} 0\n"));
        }
    }
    for (check, ready) in readyz {
        output.push_str(&format!("pandar_readyz{{check=\"{check}\"}} {ready}\n"));
    }
}

struct GroupedCount {
    labels: Vec<String>,
    count: i64,
}

async fn grouped_counts(
    database: &Database,
    table: &'static str,
    columns: &[&'static str],
) -> anyhow::Result<Vec<GroupedCount>> {
    let select = columns.join(", ");
    let sql = format!("SELECT {select}, COUNT(*) AS count FROM {table} GROUP BY {select}");
    let statement = Statement::from_string(backend(database), sql);
    let rows = database
        .sea_orm_connection()
        .query_all_raw(statement)
        .await
        .with_context(|| format!("failed to collect {table} metrics"))?;
    rows.into_iter()
        .map(|row| {
            let labels = columns
                .iter()
                .map(|column| row.try_get("", column))
                .collect::<Result<Vec<String>, _>>()?;
            let count = row.try_get::<i64>("", "count")?;
            Ok(GroupedCount { labels, count })
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()
        .map_err(anyhow::Error::from)
}

fn backend(database: &Database) -> sea_orm::DatabaseBackend {
    match database.backend() {
        DatabaseBackend::Sqlite => sea_orm::DatabaseBackend::Sqlite,
        DatabaseBackend::Postgres => sea_orm::DatabaseBackend::Postgres,
    }
}

fn escape_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
