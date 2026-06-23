use anyhow::Context;
use pandar_core::{
    CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, JobStatus, PrintStatus,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};

use crate::{
    entities::{job_artifacts, jobs, printers},
    repositories::{
        CreatePrintJob, DuplicatePrintJob, JobWithArtifact, PrintProjectFilePayload,
        RepositoryError, RepositoryResult,
        commands::inserts::{self, InsertCommand},
    },
};

pub async fn create_print_job<C>(
    connection: &C,
    input: CreatePrintJob,
) -> RepositoryResult<JobWithArtifact>
where
    C: ConnectionTrait,
{
    validate_mapping_json(&input.ams_mapping_json, "ams_mapping_json")?;
    validate_mapping_json(&input.ams_mapping2_json, "ams_mapping2_json")?;
    let serial_number = printer_for_agent(connection, &input).await?;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();

    insert_artifact(connection, &input, &now).await?;
    let payload = payload(&input, job_id, &serial_number);
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    inserts::insert(
        connection,
        InsertCommand {
            id: command_id,
            tenant_id: input.tenant_id,
            agent_id: input.agent_id,
            printer_id: Some(&input.printer_id),
            kind: "print_project_file",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await?;
    insert_job(connection, &input, job_id, command_id, &now).await?;
    build_created_job(input, job_id, command_id, now)
}

pub struct NewPrintJobFromArtifact {
    tenant_id: pandar_core::TenantId,
    printer_id: String,
    agent_id: pandar_core::AgentId,
    artifact_id: String,
    artifact_filename: String,
    artifact_content_type: String,
    artifact_size_bytes: u64,
    artifact_storage_path: String,
    plate_id: u32,
    use_ams: bool,
    flow_cali: bool,
    timelapse: bool,
    ams_mapping_json: Option<String>,
    ams_mapping2_json: Option<String>,
}

impl NewPrintJobFromArtifact {
    pub fn from_source(
        source: JobWithArtifact,
        source_payload: PrintProjectFilePayload,
        overrides: Option<DuplicatePrintJob>,
    ) -> Self {
        let overrides = overrides.unwrap_or(DuplicatePrintJob {
            printer_id: None,
            plate_id: None,
            use_ams: None,
            flow_cali: None,
            timelapse: None,
            ams_mapping_json: None,
            ams_mapping2_json: None,
        });
        Self {
            tenant_id: source.job.tenant_id,
            printer_id: overrides.printer_id.unwrap_or(source.job.printer_id),
            agent_id: source.job.agent_id,
            artifact_id: source.artifact.id,
            artifact_filename: source.artifact.filename,
            artifact_content_type: source.artifact.content_type,
            artifact_size_bytes: source.artifact.size_bytes,
            artifact_storage_path: source.artifact.storage_path,
            plate_id: overrides.plate_id.unwrap_or(source_payload.plate_id),
            use_ams: overrides.use_ams.unwrap_or(source_payload.use_ams),
            flow_cali: overrides.flow_cali.unwrap_or(source_payload.flow_cali),
            timelapse: overrides.timelapse.unwrap_or(source_payload.timelapse),
            ams_mapping_json: overrides.ams_mapping_json.or(source.job.ams_mapping_json),
            ams_mapping2_json: overrides.ams_mapping2_json.or(source.job.ams_mapping2_json),
        }
    }
}

pub async fn create_print_job_from_artifact<C>(
    connection: &C,
    input: NewPrintJobFromArtifact,
) -> RepositoryResult<JobWithArtifact>
where
    C: ConnectionTrait,
{
    validate_mapping_json(&input.ams_mapping_json, "ams_mapping_json")?;
    validate_mapping_json(&input.ams_mapping2_json, "ams_mapping2_json")?;
    let serial_number = printer_for_existing_artifact(connection, &input).await?;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();
    let payload = payload_from_existing_artifact(&input, job_id, &serial_number);
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    inserts::insert(
        connection,
        InsertCommand {
            id: command_id,
            tenant_id: input.tenant_id,
            agent_id: input.agent_id,
            printer_id: Some(&input.printer_id),
            kind: "print_project_file",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await?;
    insert_job_from_existing_artifact(connection, &input, job_id, command_id, &now).await?;
    build_job_from_existing_artifact(input, job_id, command_id, now)
}

fn validate_mapping_json(value: &Option<String>, field: &'static str) -> RepositoryResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = match field {
        "ams_mapping_json" => serde_json::from_str::<Vec<i32>>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        "ams_mapping2_json" => {
            let entries = serde_json::from_str::<
                Vec<crate::repositories::jobs::print_reports::usage::Mapping2Entry>,
            >(value)
            .with_context(|| format!("failed to validate {field}"))?;
            entries.len()
        }
        _ => unreachable!("validated mapping field should be known"),
    };
    if len > 32 {
        return Err(RepositoryError::Database(anyhow::anyhow!(
            "{field} must not contain more than 32 entries"
        )));
    }
    Ok(())
}

async fn printer_for_agent<C>(connection: &C, input: &CreatePrintJob) -> RepositoryResult<String>
where
    C: ConnectionTrait,
{
    printers::Entity::find_by_id(&input.printer_id)
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()))
        .one(connection)
        .await
        .context("failed to verify print job printer ownership")?
        .map(|printer| printer.serial_number)
        .ok_or(RepositoryError::MissingPrinter)
}

async fn printer_for_existing_artifact<C>(
    connection: &C,
    input: &NewPrintJobFromArtifact,
) -> RepositoryResult<String>
where
    C: ConnectionTrait,
{
    printers::Entity::find_by_id(&input.printer_id)
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()))
        .one(connection)
        .await
        .context("failed to verify recovered print job printer ownership")?
        .map(|printer| printer.serial_number)
        .ok_or(RepositoryError::MissingPrinter)
}

async fn insert_artifact<C>(
    connection: &C,
    input: &CreatePrintJob,
    now: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    job_artifacts::ActiveModel {
        id: Set(input.artifact_id.clone()),
        tenant_id: Set(input.tenant_id.to_string()),
        filename: Set(input.artifact_filename.clone()),
        content_type: Set(input.artifact_content_type.clone()),
        size_bytes: Set(input.artifact_size_bytes as i64),
        storage_path: Set(input.artifact_storage_path.clone()),
        created_at: Set(now.to_owned()),
    }
    .insert(connection)
    .await
    .context("failed to insert job artifact")?;
    Ok(())
}

async fn insert_job<C>(
    connection: &C,
    input: &CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    now: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    jobs::ActiveModel {
        id: Set(job_id.to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        printer_id: Set(input.printer_id.clone()),
        agent_id: Set(input.agent_id.to_string()),
        artifact_id: Set(input.artifact_id.clone()),
        command_id: Set(command_id.to_string()),
        status: Set(JobStatus::Queued.as_str().to_owned()),
        error: Set(None),
        created_at: Set(now.to_owned()),
        updated_at: Set(now.to_owned()),
        print_status: Set(PrintStatus::Pending.as_str().to_owned()),
        ams_mapping_json: Set(input.ams_mapping_json.clone()),
        ams_mapping2_json: Set(input.ams_mapping2_json.clone()),
        ..Default::default()
    }
    .insert(connection)
    .await
    .context("failed to insert print job")?;
    Ok(())
}

async fn insert_job_from_existing_artifact<C>(
    connection: &C,
    input: &NewPrintJobFromArtifact,
    job_id: JobId,
    command_id: CommandId,
    now: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    jobs::ActiveModel {
        id: Set(job_id.to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        printer_id: Set(input.printer_id.clone()),
        agent_id: Set(input.agent_id.to_string()),
        artifact_id: Set(input.artifact_id.clone()),
        command_id: Set(command_id.to_string()),
        status: Set(JobStatus::Queued.as_str().to_owned()),
        error: Set(None),
        created_at: Set(now.to_owned()),
        updated_at: Set(now.to_owned()),
        print_status: Set(PrintStatus::Pending.as_str().to_owned()),
        ams_mapping_json: Set(input.ams_mapping_json.clone()),
        ams_mapping2_json: Set(input.ams_mapping2_json.clone()),
        ..Default::default()
    }
    .insert(connection)
    .await
    .context("failed to insert recovered print job")?;
    Ok(())
}

fn payload(input: &CreatePrintJob, job_id: JobId, serial_number: &str) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: job_id.to_string(),
        artifact_id: input.artifact_id.clone(),
        printer_id: input.printer_id.clone(),
        serial_number: serial_number.to_string(),
        filename: input.artifact_filename.clone(),
        storage_path: input.artifact_storage_path.clone(),
        size_bytes: input.artifact_size_bytes,
        plate_id: input.plate_id,
        use_ams: input.use_ams,
        flow_cali: input.flow_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
    }
}

fn payload_from_existing_artifact(
    input: &NewPrintJobFromArtifact,
    job_id: JobId,
    serial_number: &str,
) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: job_id.to_string(),
        artifact_id: input.artifact_id.clone(),
        printer_id: input.printer_id.clone(),
        serial_number: serial_number.to_string(),
        filename: input.artifact_filename.clone(),
        storage_path: input.artifact_storage_path.clone(),
        size_bytes: input.artifact_size_bytes,
        plate_id: input.plate_id,
        use_ams: input.use_ams,
        flow_cali: input.flow_cali,
        timelapse: input.timelapse,
        ams_mapping_json: input.ams_mapping_json.clone(),
        ams_mapping2_json: input.ams_mapping2_json.clone(),
    }
}

fn build_created_job(
    input: CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    now: String,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        artifact: JobArtifact::from_parts(JobArtifactParts {
            id: input.artifact_id.clone(),
            tenant_id: input.tenant_id,
            filename: input.artifact_filename,
            content_type: input.artifact_content_type,
            size_bytes: input.artifact_size_bytes,
            storage_path: input.artifact_storage_path,
            created_at: now.clone(),
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job artifact")?,
        job: Job::from_parts(JobParts {
            id: job_id,
            tenant_id: input.tenant_id,
            printer_id: input.printer_id,
            agent_id: input.agent_id,
            artifact_id: input.artifact_id,
            command_id,
            status: JobStatus::Queued.as_str().to_string(),
            error: None,
            print_status: PrintStatus::Pending.as_str().to_string(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            ams_mapping_json: input.ams_mapping_json,
            ams_mapping2_json: input.ams_mapping2_json,
            filament_usage: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job")?,
    })
}

fn build_job_from_existing_artifact(
    input: NewPrintJobFromArtifact,
    job_id: JobId,
    command_id: CommandId,
    now: String,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        artifact: JobArtifact::from_parts(JobArtifactParts {
            id: input.artifact_id.clone(),
            tenant_id: input.tenant_id,
            filename: input.artifact_filename,
            content_type: input.artifact_content_type,
            size_bytes: input.artifact_size_bytes,
            storage_path: input.artifact_storage_path,
            created_at: now.clone(),
        })
        .map_err(anyhow::Error::from)
        .context("failed to build recovered print job artifact")?,
        job: Job::from_parts(JobParts {
            id: job_id,
            tenant_id: input.tenant_id,
            printer_id: input.printer_id,
            agent_id: input.agent_id,
            artifact_id: input.artifact_id,
            command_id,
            status: JobStatus::Queued.as_str().to_string(),
            error: None,
            print_status: PrintStatus::Pending.as_str().to_string(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            ams_mapping_json: input.ams_mapping_json,
            ams_mapping2_json: input.ams_mapping2_json,
            filament_usage: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(anyhow::Error::from)
        .context("failed to build recovered print job")?,
    })
}
