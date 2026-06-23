use std::{collections::HashMap, sync::Arc};

use pandar_core::{Printer, TenantId};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

use crate::{
    metrics::{MetricsState, SubscriptionGuard},
    routes::jobs::JobResponse,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: Printer },
    #[serde(rename = "job_progress")]
    JobProgress { job: Box<JobResponse> },
}

#[derive(Debug, Clone)]
pub struct PrinterEventHub {
    senders: Arc<Mutex<HashMap<String, broadcast::Sender<PrinterEvent>>>>,
    metrics: MetricsState,
}

impl PrinterEventHub {
    pub fn new() -> Self {
        Self::with_metrics(MetricsState::new())
    }

    pub fn with_metrics(metrics: MetricsState) -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            metrics,
        }
    }

    pub async fn subscribe(&self, tenant_id: TenantId) -> broadcast::Receiver<PrinterEvent> {
        self.sender(tenant_id).await.subscribe()
    }

    pub async fn track_subscription(&self, tenant_id: TenantId) -> SubscriptionGuard {
        self.metrics.subscription_started(tenant_id).await
    }

    pub async fn publish_local(&self, tenant_id: TenantId, event: PrinterEvent) {
        let sender = self.sender(tenant_id).await;
        let _ = sender.send(event);
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
