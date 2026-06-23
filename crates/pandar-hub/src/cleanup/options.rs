use anyhow::Context;
use serde::Serialize;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    DryRun,
    Execute,
}

#[derive(Debug, Clone)]
pub struct CleanupOptions {
    pub completed_jobs_days: i64,
    pub commands_days: i64,
    pub machine_events_days: i64,
    pub audit_days: i64,
    pub expired_tickets_days: i64,
    pub revoked_tokens_days: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct CleanupSummary {
    pub jobs: i64,
    pub artifacts: i64,
    pub artifact_bytes: i64,
    pub commands: i64,
    pub machine_events: i64,
    pub audit_events: i64,
    pub plugin_login_tickets: i64,
    pub tenant_tokens: i64,
    #[serde(skip)]
    pub artifact_ids: Vec<String>,
    #[serde(skip)]
    pub artifact_storage_paths: Vec<String>,
}

impl CleanupOptions {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            completed_jobs_days: retention_days("PANDAR_RETENTION_COMPLETED_JOBS_DAYS", 90)?,
            commands_days: retention_days("PANDAR_RETENTION_COMMANDS_DAYS", 90)?,
            machine_events_days: retention_days("PANDAR_RETENTION_MACHINE_EVENTS_DAYS", 30)?,
            audit_days: retention_days("PANDAR_RETENTION_AUDIT_DAYS", 365)?,
            expired_tickets_days: retention_days("PANDAR_RETENTION_EXPIRED_TICKETS_DAYS", 7)?,
            revoked_tokens_days: retention_days("PANDAR_RETENTION_REVOKED_TOKENS_DAYS", 365)?,
        })
    }
}

impl Default for CleanupOptions {
    fn default() -> Self {
        Self {
            completed_jobs_days: 90,
            commands_days: 90,
            machine_events_days: 30,
            audit_days: 365,
            expired_tickets_days: 7,
            revoked_tokens_days: 365,
        }
    }
}

pub(super) struct CleanupCutoffs {
    pub jobs: String,
    pub commands: String,
    pub machine_events: String,
    pub audit: String,
    pub plugin_tickets: String,
    pub tenant_tokens: String,
}

impl CleanupCutoffs {
    pub fn from_options(options: &CleanupOptions) -> anyhow::Result<Self> {
        let now = OffsetDateTime::now_utc();
        Ok(Self {
            jobs: cutoff(now, options.completed_jobs_days)?,
            commands: cutoff(now, options.commands_days)?,
            machine_events: cutoff(now, options.machine_events_days)?,
            audit: cutoff(now, options.audit_days)?,
            plugin_tickets: cutoff(now, options.expired_tickets_days)?,
            tenant_tokens: cutoff(now, options.revoked_tokens_days)?,
        })
    }
}

fn cutoff(now: OffsetDateTime, days: i64) -> anyhow::Result<String> {
    Ok((now - Duration::days(days)).format(&Rfc3339)?)
}

fn retention_days(name: &str, default: i64) -> anyhow::Result<i64> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<i64>()
            .with_context(|| format!("failed to parse {name}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(err).with_context(|| format!("failed to read {name}")),
    }
}
