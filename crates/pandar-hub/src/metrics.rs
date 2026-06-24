use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use pandar_core::TenantId;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct MetricsState {
    websocket_tickets: Arc<WebsocketTicketCounters>,
    websocket_subscriptions: Arc<Mutex<HashMap<String, i64>>>,
    print_reports: Arc<PrintReportCounters>,
    control_plane: Arc<ControlPlaneCounters>,
    readyz: Arc<Mutex<HashMap<&'static str, i64>>>,
}

#[derive(Debug, Default)]
struct WebsocketTicketCounters {
    issued: AtomicI64,
    consumed: AtomicI64,
    expired: AtomicI64,
    invalid: AtomicI64,
}

#[derive(Debug, Default)]
struct PrintReportCounters {
    accepted: AtomicI64,
    rejected: AtomicI64,
}

#[derive(Debug, Default)]
struct ControlPlaneCounters {
    publish_ok: AtomicI64,
    publish_failed: AtomicI64,
    receive_ok: AtomicI64,
    receive_failed: AtomicI64,
}

#[derive(Debug, Clone, Copy)]
pub enum TicketMetric {
    Issued,
    Consumed,
    Expired,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub enum PrintReportMetric {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy)]
pub enum ControlPlaneMetric {
    PublishOk,
    PublishFailed,
    ReceiveOk,
    ReceiveFailed,
}

impl MetricsState {
    pub fn new() -> Self {
        Self {
            websocket_tickets: Arc::new(WebsocketTicketCounters::default()),
            websocket_subscriptions: Arc::new(Mutex::new(HashMap::new())),
            print_reports: Arc::new(PrintReportCounters::default()),
            control_plane: Arc::new(ControlPlaneCounters::default()),
            readyz: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn record_ticket(&self, metric: TicketMetric) {
        let counter = match metric {
            TicketMetric::Issued => &self.websocket_tickets.issued,
            TicketMetric::Consumed => &self.websocket_tickets.consumed,
            TicketMetric::Expired => &self.websocket_tickets.expired,
            TicketMetric::Invalid => &self.websocket_tickets.invalid,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_print_report(&self, metric: PrintReportMetric) {
        let counter = match metric {
            PrintReportMetric::Accepted => &self.print_reports.accepted,
            PrintReportMetric::Rejected => &self.print_reports.rejected,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_control_plane(&self, metric: ControlPlaneMetric) {
        let counter = match metric {
            ControlPlaneMetric::PublishOk => &self.control_plane.publish_ok,
            ControlPlaneMetric::PublishFailed => &self.control_plane.publish_failed,
            ControlPlaneMetric::ReceiveOk => &self.control_plane.receive_ok,
            ControlPlaneMetric::ReceiveFailed => &self.control_plane.receive_failed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn subscription_started(&self, tenant_id: TenantId) -> SubscriptionGuard {
        let tenant_id_hash = tenant_id_hash(tenant_id);
        let mut subscriptions = self.websocket_subscriptions.lock().await;
        *subscriptions.entry(tenant_id_hash.clone()).or_default() += 1;
        SubscriptionGuard {
            metrics: self.clone(),
            tenant_id_hash,
        }
    }

    async fn subscription_finished(&self, tenant_id_hash: &str) {
        let mut subscriptions = self.websocket_subscriptions.lock().await;
        if let Some(count) = subscriptions.get_mut(tenant_id_hash) {
            *count -= 1;
            if *count <= 0 {
                subscriptions.remove(tenant_id_hash);
            }
        }
    }

    pub async fn set_readyz(&self, check: &'static str, ready: bool) {
        self.readyz
            .lock()
            .await
            .insert(check, if ready { 1 } else { 0 });
    }

    pub fn websocket_ticket_snapshot(&self) -> [(&'static str, i64); 4] {
        [
            (
                "issued",
                self.websocket_tickets.issued.load(Ordering::Relaxed),
            ),
            (
                "consumed",
                self.websocket_tickets.consumed.load(Ordering::Relaxed),
            ),
            (
                "expired",
                self.websocket_tickets.expired.load(Ordering::Relaxed),
            ),
            (
                "invalid",
                self.websocket_tickets.invalid.load(Ordering::Relaxed),
            ),
        ]
    }

    pub fn print_report_snapshot(&self) -> [(&'static str, i64); 2] {
        [
            (
                "accepted",
                self.print_reports.accepted.load(Ordering::Relaxed),
            ),
            (
                "rejected",
                self.print_reports.rejected.load(Ordering::Relaxed),
            ),
        ]
    }

    pub fn control_plane_snapshot(&self) -> [(&'static str, i64); 4] {
        [
            (
                "publish_ok",
                self.control_plane.publish_ok.load(Ordering::Relaxed),
            ),
            (
                "publish_failed",
                self.control_plane.publish_failed.load(Ordering::Relaxed),
            ),
            (
                "receive_ok",
                self.control_plane.receive_ok.load(Ordering::Relaxed),
            ),
            (
                "receive_failed",
                self.control_plane.receive_failed.load(Ordering::Relaxed),
            ),
        ]
    }

    pub async fn websocket_subscription_snapshot(&self) -> Vec<(String, i64)> {
        self.websocket_subscriptions
            .lock()
            .await
            .iter()
            .map(|(tenant_id_hash, count)| (tenant_id_hash.clone(), *count))
            .collect()
    }

    pub async fn readyz_snapshot(&self) -> Vec<(&'static str, i64)> {
        self.readyz
            .lock()
            .await
            .iter()
            .map(|(check, ready)| (*check, *ready))
            .collect()
    }
}

impl Default for MetricsState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct SubscriptionGuard {
    metrics: MetricsState,
    tenant_id_hash: String,
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        let metrics = self.metrics.clone();
        let tenant_id_hash = self.tenant_id_hash.clone();
        tokio::spawn(async move {
            metrics.subscription_finished(&tenant_id_hash).await;
        });
    }
}

pub fn tenant_id_hash(tenant_id: TenantId) -> String {
    let hash = Sha256::digest(tenant_id.to_string());
    hash[..8].iter().map(|byte| format!("{byte:02x}")).collect()
}
