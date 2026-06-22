use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use pandar_core::{Printer, TenantId};
use serde::Serialize;
use tokio::sync::{Mutex, broadcast};

use crate::routes::jobs::JobResponse;

const TICKET_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum PrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: Printer },
    #[serde(rename = "job_progress")]
    JobProgress { job: Box<JobResponse> },
}

#[derive(Debug, Clone)]
struct PrinterEventTicket {
    tenant_id: TenantId,
    expires_at: Instant,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedPrinterEventTicket {
    pub ticket: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct PrinterEventHub {
    senders: Arc<Mutex<HashMap<String, broadcast::Sender<PrinterEvent>>>>,
    tickets: Arc<Mutex<HashMap<String, PrinterEventTicket>>>,
}

impl PrinterEventHub {
    pub fn new() -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            tickets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn subscribe(&self, tenant_id: TenantId) -> broadcast::Receiver<PrinterEvent> {
        self.sender(tenant_id).await.subscribe()
    }

    pub async fn publish(&self, tenant_id: TenantId, event: PrinterEvent) {
        let sender = self.sender(tenant_id).await;
        let _ = sender.send(event);
    }

    pub async fn issue_ticket(&self, tenant_id: TenantId) -> IssuedPrinterEventTicket {
        let ticket = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        let expires_at = now + TICKET_TTL;
        let expires_at_text = time::OffsetDateTime::now_utc()
            .saturating_add(time::Duration::seconds(TICKET_TTL.as_secs() as i64))
            .format(&time::format_description::well_known::Rfc3339)
            .expect("UTC ticket expiry must format as RFC3339");

        let mut tickets = self.tickets.lock().await;
        tickets.retain(|_, value| value.expires_at > now);
        tickets.insert(
            ticket.clone(),
            PrinterEventTicket {
                tenant_id,
                expires_at,
            },
        );

        IssuedPrinterEventTicket {
            ticket,
            expires_at: expires_at_text,
        }
    }

    pub async fn consume_ticket(&self, tenant_id: TenantId, ticket: &str) -> bool {
        let now = Instant::now();
        let mut tickets = self.tickets.lock().await;
        tickets.retain(|_, value| value.expires_at > now);
        tickets
            .remove(ticket)
            .is_some_and(|value| value.tenant_id == tenant_id && value.expires_at > now)
    }

    async fn sender(&self, tenant_id: TenantId) -> broadcast::Sender<PrinterEvent> {
        let mut senders = self.senders.lock().await;
        senders
            .entry(tenant_id.to_string())
            .or_insert_with(|| broadcast::channel(128).0)
            .clone()
    }
}

impl Default for PrinterEventHub {
    fn default() -> Self {
        Self::new()
    }
}
