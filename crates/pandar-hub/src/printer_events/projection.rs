use std::{collections::HashMap, sync::Arc};

use pandar_core::TenantId;
use tokio::sync::{Mutex, OwnedMutexGuard, broadcast};

/// Internal notification that one printer's Studio-facing record changed.
/// Carries enough identity to emit a removal after the row no longer exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterProjectionChange {
    pub printer_id: String,
    pub serial_number: String,
}

#[derive(Debug, Clone)]
struct TenantProjectionChannel {
    sender: broadcast::Sender<PrinterProjectionChange>,
    publication: Arc<Mutex<()>>,
}

pub struct ProjectionSubscription {
    receiver: broadcast::Receiver<PrinterProjectionChange>,
    publication: Arc<Mutex<()>>,
}

impl ProjectionSubscription {
    pub async fn recv(&mut self) -> Result<PrinterProjectionChange, broadcast::error::RecvError> {
        self.receiver.recv().await
    }

    pub async fn lock_publication(&self) -> OwnedMutexGuard<()> {
        Arc::clone(&self.publication).lock_owned().await
    }

    pub fn drain_buffered(&mut self) -> Result<Vec<PrinterProjectionChange>, u64> {
        let mut buffered = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(change) => buffered.push(change),
                Err(broadcast::error::TryRecvError::Empty)
                | Err(broadcast::error::TryRecvError::Closed) => return Ok(buffered),
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => return Err(skipped),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ProjectionEventHub {
    channels: Arc<Mutex<HashMap<String, TenantProjectionChannel>>>,
    capacity: usize,
}

impl ProjectionEventHub {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
            capacity,
        }
    }

    pub(super) async fn subscribe(&self, tenant_id: TenantId) -> ProjectionSubscription {
        let channel = self.channel(tenant_id).await;
        ProjectionSubscription {
            receiver: channel.sender.subscribe(),
            publication: channel.publication,
        }
    }

    pub(super) async fn publish(&self, tenant_id: TenantId, change: PrinterProjectionChange) {
        let channel = self.channel(tenant_id).await;
        let _publication = channel.publication.lock().await;
        let _ = channel.sender.send(change);
    }

    async fn channel(&self, tenant_id: TenantId) -> TenantProjectionChannel {
        let mut channels = self.channels.lock().await;
        channels
            .entry(tenant_id.to_string())
            .or_insert_with(|| TenantProjectionChannel {
                sender: broadcast::channel(self.capacity).0,
                publication: Arc::new(Mutex::new(())),
            })
            .clone()
    }

    /// Drops tenant channels with no live receiver. Subscribers keep their
    /// own channel and publication clones, and sends to an empty channel are
    /// already dropped, so removal is unobservable; the next subscribe or
    /// publish recreates the channel on demand.
    pub(super) async fn sweep_idle_channels(&self) -> usize {
        let mut channels = self.channels.lock().await;
        let before = channels.len();
        channels.retain(|_, channel| channel.sender.receiver_count() > 0);
        before - channels.len()
    }

    #[cfg(test)]
    pub(super) async fn channel_count(&self) -> usize {
        self.channels.lock().await.len()
    }
}
