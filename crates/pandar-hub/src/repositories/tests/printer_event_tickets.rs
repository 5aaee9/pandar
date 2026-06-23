use pandar_core::TenantId;
use time::Duration;

use super::{
    Database, PrinterEventTicketConsumeResult, PrinterEventTicketRepository, TenantRepository,
};
use crate::repositories::printer_event_tickets::format_ticket_timestamp;

#[test]
fn printer_event_ticket_migrations_are_backend_equivalent() {
    let sqlite =
        include_str!("../../../migrations/sqlite/20260623030000_hub_control_plane_tickets.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260623030000_hub_control_plane_tickets.sql");

    assert_eq!(sqlite, postgres);
    assert!(sqlite.contains("CREATE TABLE printer_event_tickets"));
    assert!(sqlite.contains("tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE"));
    assert!(sqlite.contains("ticket_hash TEXT NOT NULL UNIQUE"));
    assert!(sqlite.contains("idx_printer_event_tickets_tenant_id"));
    assert!(sqlite.contains("idx_printer_event_tickets_hash"));
    assert!(sqlite.contains("idx_printer_event_tickets_expires_at"));
}

pub(super) async fn ticket_repository_semantics(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let tenant = tenants.create("ticket-acme", "Ticket Acme").await.unwrap();
    let sibling_tenant = tenants
        .create("ticket-sibling", "Ticket Sibling")
        .await
        .unwrap();
    let tickets = PrinterEventTicketRepository::new(database.clone());
    let tenant_id = tenant.id;
    let sibling_tenant_id = sibling_tenant.id;

    let issued = tickets.issue(tenant_id, "ticket-hash-1").await.unwrap();
    assert_eq!(issued.tenant_id, tenant_id);
    assert_eq!(issued.ticket_hash, "ticket-hash-1");
    assert_eq!(issued.used_at, None);
    assert!(issued.created_at <= issued.expires_at);

    let sibling = tickets
        .issue(sibling_tenant_id, "ticket-hash-2")
        .await
        .unwrap();
    assert!(matches!(
        tickets
            .consume(tenant_id, &sibling.ticket_hash)
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Invalid
    ));
    assert!(matches!(
        tickets
            .consume(sibling_tenant_id, &sibling.ticket_hash)
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Consumed(_)
    ));

    assert!(matches!(
        tickets
            .consume(tenant_id, &issued.ticket_hash)
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Consumed(_)
    ));
    assert!(matches!(
        tickets
            .consume(tenant_id, &issued.ticket_hash)
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Invalid
    ));
    assert!(matches!(
        tickets
            .consume(sibling_tenant_id, &issued.ticket_hash)
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Invalid
    ));

    let expired_at =
        format_ticket_timestamp(time::OffsetDateTime::now_utc() - Duration::seconds(1)).unwrap();
    seed_ticket(&database, tenant_id, "ticket-hash-expired", &expired_at).await;
    assert!(matches!(
        tickets
            .consume(tenant_id, "ticket-hash-expired")
            .await
            .unwrap(),
        PrinterEventTicketConsumeResult::Expired
    ));
}

async fn seed_ticket(
    database: &Database,
    tenant_id: TenantId,
    ticket_hash: &str,
    expires_at: &str,
) {
    let created_at = format_ticket_timestamp(time::OffsetDateTime::now_utc()).unwrap();
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id.to_string())
            .bind(ticket_hash)
            .bind(&created_at)
            .bind(expires_at)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                 VALUES ($1, $2, $3, $4, $5, NULL)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id.to_string())
            .bind(ticket_hash)
            .bind(&created_at)
            .bind(expires_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn sqlite_ticket_repository_semantics() {
    ticket_repository_semantics(super::sqlite_database().await).await;
}

#[tokio::test]
async fn postgres_ticket_repository_semantics() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    ticket_repository_semantics(database).await;
}
