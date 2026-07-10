use anyhow::Context;
use pandar_core::{AgentId, JobId, TenantId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};

use crate::{
    db::Database,
    repositories::{
        JobWithArtifact, MaterialPatchOutcome, PrinterHms, RepositoryError, RepositoryResult,
        printers::{PrinterLiveStatusPatch, update_live_status},
    },
};

mod correlation;
mod events;
mod state;
pub(crate) mod usage;

use correlation::{correlate_job, job_by_id, printer_for_serial};
use events::{insert_job_events, insert_printer_events};
use state::{reconciled_update, update_from_job, update_job_print};
use usage::derive_terminal_usage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPrintReport {
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial: String,
    pub task_id: Option<String>,
    pub job_id: Option<JobId>,
    pub artifact_id: Option<String>,
    pub subtask_id: Option<String>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub gcode_state: Option<String>,
    pub percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub hms: Option<Vec<PrinterHms>>,
    pub diagnostics: Vec<PrintReportDiagnostic>,
    pub printer_materials_json: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintReportDiagnostic {
    pub kind: String,
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedPrintReport {
    pub job: Option<JobWithArtifact>,
    pub material_outcome: MaterialPatchOutcome,
    pub changed: bool,
    pub inserted_job_events: bool,
    pub inserted_printer_events: bool,
}

pub async fn apply_print_report(
    database: &Database,
    input: ApplyPrintReport,
) -> RepositoryResult<AppliedPrintReport> {
    let tx = begin_print_report_transaction(database).await?;
    let applied = apply_print_report_tx(&tx, input).await?;
    tx.commit()
        .await
        .context("failed to commit print report transaction")?;
    Ok(applied)
}

async fn begin_print_report_transaction(
    database: &Database,
) -> RepositoryResult<DatabaseTransaction> {
    let connection = database.sea_orm_connection();
    connection
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: matches!(database, Database::Sqlite(_))
                .then_some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
        .context("failed to begin print report transaction")
        .map_err(Into::into)
}

async fn apply_print_report_tx<C>(
    transaction: &C,
    input: ApplyPrintReport,
) -> RepositoryResult<AppliedPrintReport>
where
    C: ConnectionTrait,
{
    let Some(printer) = printer_for_serial(transaction, &input).await? else {
        return Ok(AppliedPrintReport {
            job: None,
            material_outcome: MaterialPatchOutcome::Empty,
            changed: false,
            inserted_job_events: false,
            inserted_printer_events: false,
        });
    };
    update_live_status(
        transaction,
        &printer.id,
        PrinterLiveStatusPatch {
            task_id: input.task_id.clone(),
            subtask_id: input.subtask_id.clone(),
            progress_percent: input.percent,
            remaining_time_minutes: input.remaining_time_minutes,
            current_layer: input.current_layer,
            total_layers: input.total_layers,
            gcode_file: input.gcode_file.clone(),
            subtask_name: input.subtask_name.clone(),
            gcode_state: input.gcode_state.clone(),
            hms: input.hms.clone(),
            observed_at: input.observed_at.clone(),
        },
    )
    .await?;
    let material_outcome = crate::repositories::materials::upsert_from_patch_outcome_in_connection(
        transaction,
        crate::repositories::MaterialPatchInput {
            tenant_id: input.tenant_id,
            agent_id: input.agent_id,
            printer_id: printer.id.clone(),
            serial_number: input.serial.clone(),
            printer_materials_json: input.printer_materials_json.clone(),
        },
    )
    .await?;
    let job = correlate_job(transaction, &input, &printer).await?;
    let Some(job) = job else {
        let inserted = insert_printer_events(transaction, &input, &printer).await?;
        return Ok(AppliedPrintReport {
            job: None,
            material_outcome,
            changed: false,
            inserted_job_events: false,
            inserted_printer_events: inserted,
        });
    };

    let original = update_from_job(&job);
    let desired = reconciled_update(&original, &input);
    let changed = original != desired;
    let job_id = job.job.id;
    let wrote = if changed {
        update_job_print(transaction, &job_id, &desired).await?
    } else {
        false
    };
    let job = job_by_id(transaction, input.tenant_id, job_id).await?;
    if let Some(job) = job.as_ref()
        && matches!(
            desired.print_status.as_str(),
            "completed" | "failed" | "cancelled"
        )
    {
        let persisted = crate::entities::jobs::Entity::find_by_id(job.job.id.to_string())
            .filter(crate::entities::jobs::Column::TenantId.eq(input.tenant_id.to_string()))
            .one(transaction)
            .await
            .context("failed to reload terminal job for usage derivation")?
            .ok_or(RepositoryError::MissingJob)?;
        derive_terminal_usage(transaction, &persisted).await?;
    }
    let job = job_by_id(transaction, input.tenant_id, job_id).await?;
    let inserted_job_events = if !changed || wrote {
        if let Some(job) = job.as_ref() {
            let persisted = update_from_job(job);
            insert_job_events(transaction, &input, &printer, job, &persisted).await?
        } else {
            false
        }
    } else {
        false
    };
    Ok(AppliedPrintReport {
        job,
        material_outcome,
        changed: changed && wrote,
        inserted_job_events,
        inserted_printer_events: false,
    })
}

#[cfg(test)]
mod transaction_tests {
    use super::begin_print_report_transaction;
    use crate::db::{Database, DatabaseConfig};

    #[tokio::test]
    async fn sqlite_print_report_transaction_reserves_writer_before_queries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}",
            temp_dir.path().join("print-report.sqlite").display()
        );
        let config = DatabaseConfig::from_url(database_url).unwrap();
        let database = Database::connect(&config).await.unwrap();
        database.migrate().await.unwrap();

        let transaction = begin_print_report_transaction(&database).await.unwrap();
        let Database::Sqlite(pool) = &database else {
            panic!("expected SQLite database")
        };
        let mut competing = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout = 1")
            .execute(&mut *competing)
            .await
            .unwrap();

        let competing_begin = sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *competing)
            .await;

        assert!(
            competing_begin.is_err(),
            "print report transaction must reserve the SQLite writer lock before its first read"
        );
        transaction.rollback().await.unwrap();
    }
}
