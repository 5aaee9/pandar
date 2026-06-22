use anyhow::Context;
use pandar_core::{AgentId, JobId, TenantId};

use crate::{
    db::Database,
    repositories::{JobWithArtifact, RepositoryResult},
};

mod correlation;
mod events;
mod state;

use correlation::{
    postgres_correlate_job, postgres_job_by_id, postgres_printer_for_serial, sqlite_correlate_job,
    sqlite_job_by_id, sqlite_printer_for_serial,
};
use events::{
    postgres_insert_job_events, postgres_insert_printer_events, sqlite_insert_job_events,
    sqlite_insert_printer_events,
};
use state::{
    postgres_update_job_print, reconciled_update, sqlite_update_job_print, update_from_job,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPrintReport {
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial: String,
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
    pub diagnostics: Vec<PrintReportDiagnostic>,
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
    pub changed: bool,
    pub inserted_job_events: bool,
    pub inserted_printer_events: bool,
}

pub async fn apply_print_report(
    database: &Database,
    input: ApplyPrintReport,
) -> RepositoryResult<AppliedPrintReport> {
    match database {
        Database::Sqlite(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin SQLite print report transaction")?;
            let applied = apply_print_report_sqlite(&mut transaction, input).await;
            match applied {
                Ok(applied) => {
                    transaction
                        .commit()
                        .await
                        .context("failed to commit SQLite print report transaction")?;
                    Ok(applied)
                }
                Err(err) => {
                    transaction
                        .rollback()
                        .await
                        .context("failed to roll back SQLite print report transaction")?;
                    Err(err)
                }
            }
        }
        Database::Postgres(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin PostgreSQL print report transaction")?;
            let applied = apply_print_report_postgres(&mut transaction, input).await;
            match applied {
                Ok(applied) => {
                    transaction
                        .commit()
                        .await
                        .context("failed to commit PostgreSQL print report transaction")?;
                    Ok(applied)
                }
                Err(err) => {
                    transaction
                        .rollback()
                        .await
                        .context("failed to roll back PostgreSQL print report transaction")?;
                    Err(err)
                }
            }
        }
    }
}

async fn apply_print_report_sqlite(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: ApplyPrintReport,
) -> RepositoryResult<AppliedPrintReport> {
    let Some(printer) = sqlite_printer_for_serial(transaction, &input).await? else {
        return Ok(AppliedPrintReport {
            job: None,
            changed: false,
            inserted_job_events: false,
            inserted_printer_events: false,
        });
    };
    let job = sqlite_correlate_job(transaction, &input, &printer).await?;
    let Some(job) = job else {
        let inserted = sqlite_insert_printer_events(transaction, &input, &printer).await?;
        return Ok(AppliedPrintReport {
            job: None,
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
        sqlite_update_job_print(transaction, &job_id, &desired).await?
    } else {
        false
    };
    let job = sqlite_job_by_id(transaction, input.tenant_id, job_id).await?;
    let inserted_job_events = if !changed || wrote {
        if let Some(job) = job.as_ref() {
            let persisted = update_from_job(job);
            sqlite_insert_job_events(transaction, &input, &printer, job, &persisted).await?
        } else {
            false
        }
    } else {
        false
    };
    Ok(AppliedPrintReport {
        job,
        changed: changed && wrote,
        inserted_job_events,
        inserted_printer_events: false,
    })
}

async fn apply_print_report_postgres(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: ApplyPrintReport,
) -> RepositoryResult<AppliedPrintReport> {
    let Some(printer) = postgres_printer_for_serial(transaction, &input).await? else {
        return Ok(AppliedPrintReport {
            job: None,
            changed: false,
            inserted_job_events: false,
            inserted_printer_events: false,
        });
    };
    let job = postgres_correlate_job(transaction, &input, &printer).await?;
    let Some(job) = job else {
        let inserted = postgres_insert_printer_events(transaction, &input, &printer).await?;
        return Ok(AppliedPrintReport {
            job: None,
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
        postgres_update_job_print(transaction, &job_id, &desired).await?
    } else {
        false
    };
    let job = postgres_job_by_id(transaction, input.tenant_id, job_id).await?;
    let inserted_job_events = if !changed || wrote {
        if let Some(job) = job.as_ref() {
            let persisted = update_from_job(job);
            postgres_insert_job_events(transaction, &input, &printer, job, &persisted).await?
        } else {
            false
        }
    } else {
        false
    };
    Ok(AppliedPrintReport {
        job,
        changed: changed && wrote,
        inserted_job_events,
        inserted_printer_events: false,
    })
}
