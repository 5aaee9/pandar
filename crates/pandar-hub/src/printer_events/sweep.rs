use super::PrinterEventHub;

impl PrinterEventHub {
    /// Drops per-tenant event channels, epochs, and projection channels that
    /// no subscriber keeps alive, so tenant churn does not accumulate map
    /// entries for the hub's lifetime. Every channel is recreated on demand
    /// and an empty one is unobservable — sends to it are dropped either way
    /// and fresh subscribers only ever see messages sent after they
    /// subscribe — so this eviction cannot change any observable behavior.
    pub(crate) async fn sweep_idle_channels(&self) {
        self.senders
            .lock()
            .await
            .retain(|_, sender| sender.receiver_count() > 0);
        self.epochs
            .lock()
            .expect("printer event epoch map should not be poisoned")
            .retain(|_, epoch| epoch.receiver_count() > 0);
        self.projection.sweep_idle_channels().await;
    }

    #[cfg(test)]
    pub(crate) async fn channel_counts_for_tests(&self) -> [usize; 3] {
        let senders = self.senders.lock().await.len();
        let epochs = self
            .epochs
            .lock()
            .expect("printer event epoch map should not be poisoned")
            .len();
        [senders, epochs, self.projection.channel_count().await]
    }
}

#[cfg(test)]
mod tests {
    use pandar_core::TenantId;

    use super::*;

    #[tokio::test]
    async fn sweep_drops_channels_without_subscribers_and_keeps_live_ones() {
        let hub = PrinterEventHub::new();
        let live = TenantId::new();
        let idle = TenantId::new();

        let receiver = hub.subscribe(live).await;
        let epoch = hub.subscribe_epoch(live);
        let changes = hub.subscribe_projection_changes(live).await;
        drop(hub.subscribe(idle).await);
        drop(hub.subscribe_epoch(idle));
        drop(hub.subscribe_projection_changes(idle).await);

        hub.sweep_idle_channels().await;
        assert_eq!(hub.channel_counts_for_tests().await, [1, 1, 1]);

        drop(receiver);
        drop(epoch);
        drop(changes);
        hub.sweep_idle_channels().await;
        assert_eq!(hub.channel_counts_for_tests().await, [0, 0, 0]);

        // Recreated channels keep serving new subscribers unchanged.
        let receiver = hub.subscribe(live).await;
        assert_eq!(hub.channel_counts_for_tests().await, [1, 0, 0]);
        drop(receiver);
    }

    #[tokio::test]
    async fn sweep_keeps_channels_for_slow_but_alive_subscribers() {
        let hub = PrinterEventHub::new();
        let tenant = TenantId::new();
        let receiver = hub.subscribe(tenant).await;
        // A receiver nobody polls is still a live subscriber.
        hub.sweep_idle_channels().await;
        assert_eq!(hub.channel_counts_for_tests().await, [1, 0, 0]);
        drop(receiver);
    }
}
