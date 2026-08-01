use crate::{
    entities::printers,
    repositories::{
        CreatePrintJob, DuplicatePrintJob, JobWithArtifact, PrintProjectFilePayload,
        RepositoryError, RepositoryResult,
        commands::inserts::{self as command_inserts, InsertCommand},
    },
};
use anyhow::Context;
use pandar_core::{AgentId, CommandId, JobId, PrintCalibrationMode, StudioPrintMetadata};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

mod build;
mod inserts;
mod payload;
mod validation;
use build::{build_created_job, build_job_from_existing_artifact};
use inserts::{insert_artifact, insert_job, insert_job_from_existing_artifact};
use payload::{payload, payload_from_existing_artifact};
use validation::validate_mapping_json;

pub async fn create_print_job<C>(
    connection: &C,
    input: CreatePrintJob,
    studio_metadata: Option<StudioPrintMetadata>,
) -> RepositoryResult<JobWithArtifact>
where
    C: ConnectionTrait,
{
    validate_mapping_json(&input.ams_mapping_json, "ams_mapping_json")?;
    validate_mapping_json(&input.ams_mapping2_json, "ams_mapping2_json")?;
    validate_mapping_json(&input.ams_mapping_info_json, "ams_mapping_info_json")?;
    let serial_number = printer_for_agent(connection, &input).await?;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();
    let studio_submission_id = super::studio_ids::allocate(connection, input.tenant_id).await?;
    let studio_metadata_json = studio_metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize Studio print metadata")?;
    insert_artifact(connection, &input, &now).await?;
    let payload = payload(
        &input,
        job_id,
        &serial_number,
        studio_submission_id,
        studio_metadata,
    );
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    command_inserts::insert(
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
    insert_job(
        connection,
        &input,
        job_id,
        command_id,
        studio_submission_id,
        studio_metadata_json.as_deref(),
        &now,
    )
    .await?;
    build_created_job(
        input,
        job_id,
        command_id,
        studio_submission_id,
        studio_metadata_json,
        now,
    )
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
    artifact_metadata_json: Option<String>,
    plate_id: u32,
    use_ams: bool,
    auto_bed_leveling: PrintCalibrationMode,
    bed_leveling: bool,
    flow_cali: bool,
    auto_flow_cali: PrintCalibrationMode,
    auto_offset_cali: PrintCalibrationMode,
    timelapse: bool,
    ams_mapping_json: Option<String>,
    ams_mapping2_json: Option<String>,
    ams_mapping_info_json: Option<String>,
    studio_metadata: Option<StudioPrintMetadata>,
}

impl NewPrintJobFromArtifact {
    pub fn from_source(
        source: JobWithArtifact,
        source_payload: PrintProjectFilePayload,
        overrides: Option<DuplicatePrintJob>,
    ) -> Self {
        let overrides = overrides.unwrap_or_default();
        let preserve_mappings = !overrides.replace_ams_mappings;
        Self {
            tenant_id: source.job.tenant_id,
            printer_id: overrides.printer_id.unwrap_or(source.job.printer_id),
            agent_id: source.job.agent_id,
            artifact_id: source.artifact.id,
            artifact_filename: source.artifact.filename,
            artifact_content_type: source.artifact.content_type,
            artifact_size_bytes: source.artifact.size_bytes,
            artifact_storage_path: source.artifact.storage_path,
            artifact_metadata_json: source.artifact.metadata_json,
            plate_id: overrides.plate_id.unwrap_or(source_payload.plate_id),
            use_ams: overrides.use_ams.unwrap_or(source_payload.use_ams),
            bed_leveling: overrides
                .bed_leveling
                .unwrap_or(source_payload.bed_leveling),
            auto_bed_leveling: overrides
                .auto_bed_leveling
                .unwrap_or(source_payload.auto_bed_leveling),
            flow_cali: overrides.flow_cali.unwrap_or(source_payload.flow_cali),
            auto_flow_cali: overrides
                .auto_flow_cali
                .unwrap_or(source_payload.auto_flow_cali),
            auto_offset_cali: overrides
                .auto_offset_cali
                .unwrap_or(source_payload.auto_offset_cali),
            timelapse: overrides.timelapse.unwrap_or(source_payload.timelapse),
            ams_mapping_json: overrides.ams_mapping_json.or_else(|| {
                preserve_mappings
                    .then_some(source.job.ams_mapping_json)
                    .flatten()
            }),
            ams_mapping2_json: overrides.ams_mapping2_json.or_else(|| {
                preserve_mappings
                    .then_some(source.job.ams_mapping2_json)
                    .flatten()
            }),
            ams_mapping_info_json: overrides.ams_mapping_info_json.or_else(|| {
                preserve_mappings
                    .then_some(source.job.ams_mapping_info_json)
                    .flatten()
            }),
            studio_metadata: source_payload.studio_metadata.clone(),
        }
    }
}

pub async fn create_print_job_from_artifact<C>(
    connection: &C,
    mut input: NewPrintJobFromArtifact,
) -> RepositoryResult<JobWithArtifact>
where
    C: ConnectionTrait,
{
    validate_mapping_json(&input.ams_mapping_json, "ams_mapping_json")?;
    validate_mapping_json(&input.ams_mapping2_json, "ams_mapping2_json")?;
    validate_mapping_json(&input.ams_mapping_info_json, "ams_mapping_info_json")?;
    let (serial_number, agent_id, model) =
        printer_for_existing_artifact(connection, &input).await?;
    if model
        .as_deref()
        .and_then(pandar_core::compatibility::normalize_model)
        .as_deref()
        == Some("H2C")
        && !input.studio_metadata.as_ref().is_some_and(|metadata| {
            pandar_core::valid_h2c_nozzle_mapping(metadata.nozzle_mapping())
        })
    {
        return Err(RepositoryError::H2cNozzleMappingRequired);
    }
    input.agent_id = agent_id;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();
    let studio_submission_id = super::studio_ids::allocate(connection, input.tenant_id).await?;
    let studio_metadata_json = input
        .studio_metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize recovered Studio print metadata")?;
    let payload = payload_from_existing_artifact(
        &input,
        job_id,
        &serial_number,
        studio_submission_id,
        input.studio_metadata.clone(),
    );
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    command_inserts::insert(
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
    insert_job_from_existing_artifact(
        connection,
        &input,
        job_id,
        command_id,
        studio_submission_id,
        studio_metadata_json.as_deref(),
        &now,
    )
    .await?;
    build_job_from_existing_artifact(
        input,
        job_id,
        command_id,
        studio_submission_id,
        studio_metadata_json,
        now,
    )
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
) -> RepositoryResult<(String, AgentId, Option<String>)>
where
    C: ConnectionTrait,
{
    printers::Entity::find_by_id(&input.printer_id)
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to verify recovered print job printer ownership")?
        .map(|printer| {
            let agent_id = AgentId::parse(&printer.agent_id).map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>((printer.serial_number, agent_id, printer.model))
        })
        .transpose()?
        .ok_or(RepositoryError::MissingPrinter)
}
