use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::Database,
    entities::printer_event_tickets,
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

        printer_event_tickets::ActiveModel {
            id: Set(ticket.id.clone()),
            tenant_id: Set(ticket.tenant_id.to_string()),
            ticket_hash: Set(ticket.ticket_hash.clone()),
            created_at: Set(ticket.created_at.clone()),
            expires_at: Set(ticket.expires_at.clone()),
            used_at: Set(None),
        }
        .insert(&self.database.sea_orm_connection())
        .await
        .context("failed to insert printer event ticket")?;

        Ok(ticket)
    }

    pub async fn consume(
        &self,
        tenant_id: TenantId,
        ticket_hash: &str,
    ) -> RepositoryResult<PrinterEventTicketConsumeResult> {
        let now = ticket_timestamp_now()?;
        let connection = self.database.sea_orm_connection();
        let updated = printer_event_tickets::Entity::update_many()
            .col_expr(
                printer_event_tickets::Column::UsedAt,
                Expr::value(now.clone()),
            )
            .filter(printer_event_tickets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_event_tickets::Column::TicketHash.eq(ticket_hash))
            .filter(printer_event_tickets::Column::UsedAt.is_null())
            .filter(printer_event_tickets::Column::ExpiresAt.gt(now.clone()))
            .exec(&connection)
            .await
            .context("failed to consume printer event ticket")?
            .rows_affected;

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

        let expired_unused = printer_event_tickets::Entity::find()
            .filter(printer_event_tickets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_event_tickets::Column::TicketHash.eq(ticket_hash))
            .filter(printer_event_tickets::Column::UsedAt.is_null())
            .filter(printer_event_tickets::Column::ExpiresAt.lte(now))
            .one(&connection)
            .await
            .context("failed to check expired printer event ticket")?
            .is_some();

        if expired_unused {
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
        printer_event_tickets::Entity::find()
            .filter(printer_event_tickets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_event_tickets::Column::TicketHash.eq(ticket_hash))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load printer event ticket")?
            .map(ticket_from_model)
            .transpose()
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

fn ticket_from_model(
    model: printer_event_tickets::Model,
) -> RepositoryResult<IssuedPrinterEventTicket> {
    Ok(IssuedPrinterEventTicket {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        ticket_hash: model.ticket_hash,
        created_at: model.created_at,
        expires_at: model.expires_at,
        used_at: model.used_at,
    })
}
