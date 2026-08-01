mod adapters;
mod agents;
mod audit;
mod auth;
mod commands;
mod jobs;
mod materials;
pub(crate) mod printer_event_tickets;
mod printers;
mod tenants;

#[cfg(test)]
pub(crate) use agents::current_transaction_pause;
pub use agents::{AGENT_CREDENTIAL_PREFIX, AgentRepository, begin_current_agent_transaction};
pub use audit::{
    AuditActor, AuditEvent, AuditEventListQuery, AuditEventRepository, RecordAuditEvent,
};
#[cfg(test)]
pub(crate) use auth::no_auth_session_test_pause;
pub use auth::{
    AcceptedJoinLink, ApiToken, AuthRepository, AuthenticatedPrincipal, AuthenticatedTenantToken,
    AuthenticatedUser, ExternalIdentityProfile, ExternalMembership, JoinLink,
    JoinLinkWithPlaintext, NoAuthPluginSession, NoAuthPluginSessionOutcome, PluginLoginTicket,
    PluginLoginTicketExchange, PluginLoginTicketWithPlaintext, TenantToken, TenantTokenScope,
    TenantTokenWithPlaintext, User, UserIdentity, UserRole,
};
#[cfg(test)]
pub(crate) use commands::printer_operation_ownership_pause;
pub use commands::{
    CommandRepository, DiagnosePrinterPayload, DiscoverPrintersPayload, FirmwareCommandOwner,
    FirmwareControlPayload, FirmwarePersistedPhase, FirmwarePersistedResult,
    FirmwareRefreshPayload, LinkPrinterPayload, PersistedLivePrinterOperation, PrintErrorAction,
    PrintProjectFilePayload, PrinterAxis, PrinterAxisMovement, PrinterOperationKind,
    PrinterOperationPayload, RefreshPrinterMaterialsPayload, ReloadPrinterConnectionPayload,
    WebPrintErrorRecovery,
};
#[cfg(test)]
pub(crate) use jobs::studio_task_test_pause;
pub use jobs::{
    AgentArtifactAccess, AppliedPrintReport, ApplyPrintReport, ArtifactQuotaLimits,
    ClearJobsOutcome, CreatePrintJob, DuplicatePrintJob, JobRepository, JobWithArtifact,
    PrintReportDiagnostic, StudioTaskPage, StudioTaskQuery, StudioTaskStatus,
};
pub(crate) use materials::CurrentMaterialPatchOutcome;
pub use materials::{
    MaterialJsonValue, MaterialPatchInput, MaterialPatchOutcome, MaterialRepository,
    MaterialSnapshot,
};
pub use printer_event_tickets::{
    IssuedPrinterEventTicket, PrinterEventTicketConsumeResult, PrinterEventTicketRepository,
};
pub use printers::{
    DeviceFeatureUpdateOutcome, PrinterFirmwareUpdateOutcome, PrinterHms, PrinterLiveStatus,
    PrinterRepository, PrinterSnapshotUpsert, PrinterWithLiveStatus,
};
pub use tenants::TenantRepository;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("tenant slug already exists")]
    DuplicateTenantSlug,
    #[error("agent name already exists for tenant")]
    DuplicateAgentName,
    #[error("api token name already exists for tenant")]
    DuplicateApiTokenName,
    #[error("api token hash already exists")]
    DuplicateApiTokenHash,
    #[error("tenant token hash already exists")]
    DuplicateTenantTokenHash,
    #[error("plugin login ticket hash already exists")]
    DuplicatePluginLoginTicketHash,
    #[error("join link hash already exists")]
    DuplicateJoinLinkHash,
    #[error("user email already exists for tenant")]
    DuplicateUserEmail,
    #[error("external identity already exists for tenant")]
    DuplicateExternalIdentity,
    #[error("external identity provider already linked to user")]
    DuplicateUserExternalIdentity,
    #[error("tenant not found")]
    MissingTenant,
    #[error("user not found")]
    MissingUser,
    #[error("cannot remove the last tenant admin")]
    LastTenantAdmin,
    #[error("api token not found")]
    MissingApiToken,
    #[error("tenant token not found")]
    MissingTenantToken,
    #[error("plugin login ticket not found")]
    MissingPluginLoginTicket,
    #[error("invalid join link")]
    InvalidJoinLink,
    #[error("join link email mismatch")]
    JoinLinkEmailMismatch,
    #[error("agent not found")]
    MissingAgent,
    #[error("agent is online")]
    AgentOnline,
    #[error("agent session is no longer current")]
    AgentSessionNotCurrent,
    #[error("printer not found")]
    MissingPrinter,
    #[error("command not found")]
    MissingCommand,
    #[error("job not found")]
    MissingJob,
    #[error("tenant artifact quota exceeded")]
    ArtifactQuotaExceeded,
    #[error("job cannot be deleted while it may still be active")]
    JobNotClearable,
    #[error("command belongs to a different tenant or agent")]
    CommandOwnershipMismatch,
    #[error("cannot {action} command from {from}")]
    InvalidCommandTransition { from: String, action: &'static str },
    #[error("invalid persisted agent status: {0}")]
    InvalidPersistedStatus(String),
    #[error("invalid persisted command status: {0}")]
    InvalidPersistedCommandStatus(String),
    #[error("invalid persisted job status: {0}")]
    InvalidPersistedJobStatus(String),
    #[error("invalid persisted print status: {0}")]
    InvalidPersistedPrintStatus(String),
    #[error("invalid persisted artifact metadata: {0:#}")]
    InvalidPersistedArtifactMetadata(anyhow::Error),
    #[error("invalid persisted Studio metadata: {0:#}")]
    InvalidPersistedStudioMetadata(anyhow::Error),
    #[error("invalid persisted user role: {0}")]
    InvalidPersistedUserRole(String),
    #[error("invalid tenant token scope: {0}")]
    InvalidTokenScope(String),
    #[error("invalid plugin redirect URL")]
    InvalidPluginRedirectUrl,
    #[error("print job cannot be retried safely")]
    RetryNotSafe,
    #[error("H2C print requires a Studio-provided physical nozzle mapping")]
    H2cNozzleMappingRequired,
    #[error("print job cannot be reprinted")]
    ReprintNotAllowed,
    #[error("Studio submission id range is exhausted for tenant")]
    StudioSubmissionIdExhausted,
    #[error("Studio print cancellation is too late because dispatch already started")]
    StudioCancellationTooLate,
    #[error("printer control is unavailable for this printer")]
    PrinterControlUnavailable,
    #[error("invalid printer control")]
    InvalidPrinterControl,
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

pub(crate) fn hash_secret(token: &str) -> String {
    auth::hash_token(token)
}

pub(crate) fn generate_secret(prefix: &str) -> String {
    auth::secrets::generate_secret(prefix)
}

#[cfg(test)]
pub(crate) fn hash_token_for_test(token: &str) -> String {
    auth::hash_token(token)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_helpers {
    use anyhow::Context;
    use pandar_core::{AgentId, TenantId};
    use sea_orm::{ActiveValue::Set, EntityTrait};

    use crate::{
        db::Database,
        entities::{commands, printers},
    };

    pub(crate) async fn insert_printer_fixture(
        database: &Database,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> anyhow::Result<String> {
        insert_printer_fixture_with_model(database, tenant_id, agent_id, None).await
    }

    pub(crate) async fn insert_printer_fixture_with_model(
        database: &Database,
        tenant_id: TenantId,
        agent_id: AgentId,
        model: Option<&str>,
    ) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let insert = printers::Entity::insert(printers::ActiveModel {
            id: Set(id.clone()),
            tenant_id: Set(tenant_id.to_string()),
            agent_id: Set(agent_id.to_string()),
            serial_number: Set(format!("serial-{id}")),
            name: Set("Fixture Printer".to_owned()),
            model: Set(model.map(str::to_owned)),
            status: Set("offline".to_owned()),
            last_seen_at: Set(Some("2026-06-20T00:00:00Z".to_owned())),
            created_at: Set("2026-06-20T00:00:00Z".to_owned()),
            ..Default::default()
        });
        insert
            .exec_without_returning(&database.sea_orm_connection())
            .await
            .context("failed to insert printer fixture")?;

        Ok(id)
    }

    pub(crate) async fn insert_command_fixture(
        database: &Database,
        tenant_id: TenantId,
        agent_id: AgentId,
        printer_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let id = format!("command-{agent_id}");
        let now = "2026-06-20T00:00:00Z";
        let insert = commands::Entity::insert(commands::ActiveModel {
            id: Set(id),
            tenant_id: Set(tenant_id.to_string()),
            agent_id: Set(agent_id.to_string()),
            printer_id: Set(printer_id.map(str::to_owned)),
            kind: Set("sync".to_owned()),
            status: Set("queued".to_owned()),
            payload_json: Set("{}".to_owned()),
            error: Set(None),
            created_at: Set(now.to_owned()),
            updated_at: Set(now.to_owned()),
            ..Default::default()
        });
        insert
            .exec_without_returning(&database.sea_orm_connection())
            .await
            .context("failed to insert command fixture")?;

        Ok(())
    }
}
