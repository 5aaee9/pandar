use anyhow::Context;
use pandar_core::TenantId;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

const PRINTER_EVENT_TICKET_TTL: Duration = Duration::seconds(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedPrinterEventTicket {
    pub id: String,
    pub tenant_id: TenantId,
    pub ticket_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrinterEventTicketConsumeResult {
    Consumed(IssuedPrinterEventTicket),
    Expired,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct PrinterEventTicketRepository {
    database: Database,
}

impl PrinterEventTicketRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn issue(
        &self,
        tenant_id: TenantId,
        ticket_hash: impl Into<String>,
    ) -> RepositoryResult<IssuedPrinterEventTicket> {
        let now_dt = OffsetDateTime::now_utc();
        let ticket = IssuedPrinterEventTicket {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            ticket_hash: ticket_hash.into(),
            created_at: format_ticket_timestamp(now_dt)?,
            expires_at: format_ticket_timestamp(now_dt + PRINTER_EVENT_TICKET_TTL)?,
            used_at: None,
        };

        match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                )
                .bind(&ticket.id)
                .bind(ticket.tenant_id.to_string())
                .bind(&ticket.ticket_hash)
                .bind(&ticket.created_at)
                .bind(&ticket.expires_at)
                .execute(pool)
                .await
                .context("failed to insert SQLite printer event ticket")?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                     VALUES ($1, $2, $3, $4, $5, NULL)",
                )
                .bind(&ticket.id)
                .bind(ticket.tenant_id.to_string())
                .bind(&ticket.ticket_hash)
                .bind(&ticket.created_at)
                .bind(&ticket.expires_at)
                .execute(pool)
                .await
                .context("failed to insert PostgreSQL printer event ticket")?;
            }
        }

        Ok(ticket)
    }

    pub async fn consume(
        &self,
        tenant_id: TenantId,
        ticket_hash: &str,
    ) -> RepositoryResult<PrinterEventTicketConsumeResult> {
        let now = ticket_timestamp_now()?;
        let tenant_id_text = tenant_id.to_string();
        let updated = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE printer_event_tickets
                     SET used_at = ?1
                     WHERE tenant_id = ?2 AND ticket_hash = ?3 AND used_at IS NULL AND expires_at > ?1",
                )
                .bind(&now)
                .bind(&tenant_id_text)
                .bind(ticket_hash)
                .execute(pool)
                .await
                .context("failed to consume SQLite printer event ticket")?
                .rows_affected()
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE printer_event_tickets
                     SET used_at = $1
                     WHERE tenant_id = $2 AND ticket_hash = $3 AND used_at IS NULL AND expires_at > $1",
                )
                .bind(&now)
                .bind(&tenant_id_text)
                .bind(ticket_hash)
                .execute(pool)
                .await
                .context("failed to consume PostgreSQL printer event ticket")?
                .rows_affected()
            }
        };

        if updated == 1 {
            let ticket = self
                .find_for_tenant_and_hash(tenant_id, ticket_hash)
                .await?
                .ok_or_else(|| {
                    RepositoryError::Database(anyhow::anyhow!(
                        "consumed printer event ticket disappeared"
                    ))
                })?;
            return Ok(PrinterEventTicketConsumeResult::Consumed(ticket));
        }

        if self
            .find_expired_unused_for_tenant_and_hash(&tenant_id_text, ticket_hash, &now)
            .await?
        {
            Ok(PrinterEventTicketConsumeResult::Expired)
        } else {
            Ok(PrinterEventTicketConsumeResult::Invalid)
        }
    }

    async fn find_for_tenant_and_hash(
        &self,
        tenant_id: TenantId,
        ticket_hash: &str,
    ) -> RepositoryResult<Option<IssuedPrinterEventTicket>> {
        let tenant_id_text = tenant_id.to_string();
        match &self.database {
            Database::Sqlite(pool) => {
                let row =
                    sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
                        "SELECT id, tenant_id, ticket_hash, created_at, expires_at, used_at
                     FROM printer_event_tickets
                     WHERE tenant_id = ?1 AND ticket_hash = ?2",
                    )
                    .bind(&tenant_id_text)
                    .bind(ticket_hash)
                    .fetch_optional(pool)
                    .await
                    .context("failed to load SQLite printer event ticket")?;
                row.map(ticket_from_values_tuple).transpose()
            }
            Database::Postgres(pool) => {
                let row =
                    sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
                        "SELECT id, tenant_id, ticket_hash, created_at, expires_at, used_at
                     FROM printer_event_tickets
                     WHERE tenant_id = $1 AND ticket_hash = $2",
                    )
                    .bind(&tenant_id_text)
                    .bind(ticket_hash)
                    .fetch_optional(pool)
                    .await
                    .context("failed to load PostgreSQL printer event ticket")?;
                row.map(ticket_from_values_tuple).transpose()
            }
        }
    }

    async fn find_expired_unused_for_tenant_and_hash(
        &self,
        tenant_id: &str,
        ticket_hash: &str,
        now: &str,
    ) -> RepositoryResult<bool> {
        let exists = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(
                        SELECT 1 FROM printer_event_tickets
                        WHERE tenant_id = ?1 AND ticket_hash = ?2 AND used_at IS NULL AND expires_at <= ?3
                    )",
                )
                .bind(tenant_id)
                .bind(ticket_hash)
                .bind(now)
                .fetch_one(pool)
                .await
                .context("failed to check expired SQLite printer event ticket")?
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(
                        SELECT 1 FROM printer_event_tickets
                        WHERE tenant_id = $1 AND ticket_hash = $2 AND used_at IS NULL AND expires_at <= $3
                    )",
                )
                .bind(tenant_id)
                .bind(ticket_hash)
                .bind(now)
                .fetch_one(pool)
                .await
                .context("failed to check expired PostgreSQL printer event ticket")?
            }
        };

        Ok(exists)
    }
}

pub(super) fn format_ticket_timestamp(value: OffsetDateTime) -> RepositoryResult<String> {
    value
        .format(&Rfc3339)
        .context("failed to format printer event ticket timestamp")
        .map_err(RepositoryError::from)
}

pub(super) fn ticket_timestamp_now() -> RepositoryResult<String> {
    format_ticket_timestamp(OffsetDateTime::now_utc())
}

fn ticket_from_values_tuple(
    row: (String, String, String, String, String, Option<String>),
) -> RepositoryResult<IssuedPrinterEventTicket> {
    ticket_from_values(row.0, row.1, row.2, row.3, row.4, row.5)
}

fn ticket_from_values(
    id: String,
    tenant_id: String,
    ticket_hash: String,
    created_at: String,
    expires_at: String,
    used_at: Option<String>,
) -> RepositoryResult<IssuedPrinterEventTicket> {
    Ok(IssuedPrinterEventTicket {
        id,
        tenant_id: TenantId::parse(&tenant_id).map_err(anyhow::Error::from)?,
        ticket_hash,
        created_at,
        expires_at,
        used_at,
    })
}
