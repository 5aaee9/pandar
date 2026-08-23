mod work;

use super::*;
use crate::connection::{AuthDisposition, ConnectionState, Reachability};

const ABI_CONNECT_FAILED: i32 = -2;
impl PluginStudioDeliveryResult {
    fn unavailable() -> Self {
        Self {
            status: ABI_CONNECT_FAILED,
            ticket: 0,
            local_generation: 0,
            account_epoch: 0,
            cache_generation: 0,
        }
    }
}

impl StudioState {
    fn issue(
        &mut self,
        kind: DeliveryKind,
        dev_id: String,
        account_epoch: u64,
        printer_epoch: u64,
    ) -> PluginStudioDeliveryResult {
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        let local_generation = match kind {
            DeliveryKind::Message {
                local_generation, ..
            } => local_generation,
            DeliveryKind::LocalConnect { generation }
            | DeliveryKind::LocalOffline { generation }
            | DeliveryKind::LocalLost { generation } => generation,
            DeliveryKind::ConnectedSignal { .. } | DeliveryKind::CloudOffline { .. } => 0,
        };
        self.issued.insert(
            self.next_ticket,
            StudioDelivery {
                kind,
                dev_id,
                account_epoch,
                printer_epoch,
                cache_generation: self.cache_generation,
                claimed: false,
            },
        );
        PluginStudioDeliveryResult {
            status: 0,
            ticket: self.next_ticket,
            local_generation,
            account_epoch,
            cache_generation: self.cache_generation,
        }
    }
}

impl ConnectionState {
    pub(super) fn studio_eligible(&self, dev_id: &str) -> bool {
        !self.studio.account_transition_pending
            && self.reachability == Reachability::Connected
            && self.auth != AuthDisposition::Rejected
            && self.printers_fresh
            && self
                .printers
                .get(dev_id)
                .is_some_and(|printer| printer.online)
    }

    fn studio_payload(&self, dev_id: &str, body: String) -> Option<StudioPayload> {
        self.printers.get(dev_id).map(|printer| StudioPayload {
            dev_id: dev_id.to_owned(),
            body,
            printer_id: printer.pandar_printer_id.clone(),
            model: printer.model.clone().unwrap_or_default(),
        })
    }

    pub(super) fn prepare_connected(
        &mut self,
        dev_id: String,
        now_ms: u64,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        if !self.studio_eligible(&dev_id)
            || !self.studio.listeners.printer_connected
            || !self.studio.cloud_target(&dev_id)
            || self.studio.cloud_initialized.contains(&dev_id)
            || self
                .studio
                .connected_notifications
                .get(&dev_id)
                .is_some_and(|previous| now_ms.saturating_sub(*previous) < CONNECTED_DEBOUNCE_MS)
        {
            return (PluginStudioDeliveryResult::unavailable(), None);
        }
        let payload = self.studio_payload(&dev_id, format!("tunnel/{dev_id}"));
        self.studio
            .connected_notifications
            .insert(dev_id.clone(), now_ms);
        let delivery = self.studio.issue(
            DeliveryKind::ConnectedSignal {
                notification_ms: now_ms,
            },
            dev_id,
            self.account_epoch,
            self.printer_epoch,
        );
        (delivery, payload)
    }

    pub(super) fn prepare_message(
        &mut self,
        tunnel: i32,
        dev_id: String,
        local_generation: u64,
        initialize_cloud: bool,
        expected_cache_generation: u64,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        let expected_cache_matches = expected_cache_generation == 0
            || expected_cache_generation == self.studio.cache_generation;
        let listener_ready = match tunnel {
            CLOUD_TUNNEL => {
                self.studio.listeners.cloud_message && self.studio.cloud_target(&dev_id)
            }
            LOCAL_TUNNEL => {
                self.studio.listeners.local_message
                    && self.studio.local.connected
                    && self.studio.local.target.as_deref() == Some(&dev_id)
                    && self.studio.local.generation == local_generation
            }
            _ => false,
        };
        if !expected_cache_matches || !listener_ready || !self.studio_eligible(&dev_id) {
            return (PluginStudioDeliveryResult::unavailable(), None);
        }
        let payload = self
            .printers
            .get(&dev_id)
            .and_then(|printer| self.studio_payload(&dev_id, printer.status_report.clone()));
        let delivery = self.studio.issue(
            DeliveryKind::Message {
                tunnel,
                local_generation,
                initialize_cloud,
            },
            dev_id,
            self.account_epoch,
            self.printer_epoch,
        );
        (delivery, payload)
    }

    pub(super) fn connect_local(
        &mut self,
        dev_id: String,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        if !self.studio.listeners.local_connected || !self.studio_eligible(&dev_id) {
            return (PluginStudioDeliveryResult::unavailable(), None);
        }
        let model = self
            .printers
            .get(&dev_id)
            .and_then(|printer| printer.model.as_deref())
            .unwrap_or_default();
        let body = crate::studio_status::local_connect_json(&dev_id, model);
        let payload = self.studio_payload(&dev_id, body);
        self.studio.local.generation = self.studio.local.generation.wrapping_add(1).max(1);
        self.studio.local.target = Some(dev_id.clone());
        self.studio.local.connected = false;
        let generation = self.studio.local.generation;
        let delivery = self.studio.issue(
            DeliveryKind::LocalConnect { generation },
            dev_id,
            self.account_epoch,
            self.printer_epoch,
        );
        (delivery, payload)
    }

    fn delivery_valid(&self, delivery: &StudioDelivery) -> bool {
        if delivery.account_epoch != self.account_epoch
            || delivery.cache_generation != self.studio.cache_generation
        {
            return false;
        }
        match delivery.kind {
            DeliveryKind::ConnectedSignal { notification_ms } => {
                self.studio_eligible(&delivery.dev_id)
                    && self.studio.listeners.printer_connected
                    && self.studio.cloud_target(&delivery.dev_id)
                    && !self.studio.cloud_initialized.contains(&delivery.dev_id)
                    && self.studio.connected_notifications.get(&delivery.dev_id)
                        == Some(&notification_ms)
            }
            DeliveryKind::Message {
                tunnel,
                local_generation,
                ..
            } => {
                delivery.printer_epoch == self.printer_epoch
                    && self.studio_eligible(&delivery.dev_id)
                    && match tunnel {
                        CLOUD_TUNNEL => {
                            self.studio.listeners.cloud_message
                                && self.studio.cloud_target(&delivery.dev_id)
                        }
                        LOCAL_TUNNEL => {
                            self.studio.listeners.local_message
                                && self.studio.local.connected
                                && self.studio.local.target.as_deref()
                                    == Some(delivery.dev_id.as_str())
                                && self.studio.local.generation == local_generation
                        }
                        _ => false,
                    }
            }
            DeliveryKind::LocalConnect { generation } => {
                delivery.printer_epoch == self.printer_epoch
                    && self.studio_eligible(&delivery.dev_id)
                    && self.studio.listeners.local_connected
                    && !self.studio.local.connected
                    && self.studio.local.target.as_deref() == Some(delivery.dev_id.as_str())
                    && self.studio.local.generation == generation
            }
            DeliveryKind::CloudOffline { forced } => {
                (forced || !self.studio_eligible(&delivery.dev_id))
                    && self.studio.listeners.cloud_message
                    && self.studio.cloud_target(&delivery.dev_id)
            }
            DeliveryKind::LocalOffline { generation } => {
                !self.studio_eligible(&delivery.dev_id)
                    && self.studio.listeners.local_message
                    && self.studio.local.target.is_none()
                    && self.studio.local.generation == generation
            }
            DeliveryKind::LocalLost { generation } => {
                !self.studio_eligible(&delivery.dev_id)
                    && self.studio.listeners.local_connected
                    && self.studio.local.target.is_none()
                    && self.studio.local.generation == generation
            }
        }
    }

    fn rollback_delivery(&mut self, delivery: &StudioDelivery) {
        match delivery.kind {
            DeliveryKind::ConnectedSignal { notification_ms } => {
                if self.studio.connected_notifications.get(&delivery.dev_id)
                    == Some(&notification_ms)
                {
                    self.studio.connected_notifications.remove(&delivery.dev_id);
                }
            }
            DeliveryKind::LocalConnect { generation }
                if self.studio.local.target.as_deref() == Some(delivery.dev_id.as_str())
                    && self.studio.local.generation == generation =>
            {
                self.studio.local.target = None;
                self.studio.local.connected = false;
                self.studio.local.generation = generation.wrapping_add(1).max(1);
            }
            _ => {}
        }
    }

    pub(super) fn complete_delivery(&mut self, ticket: u64, delivered: bool) -> bool {
        let Some(delivery) = self.studio.issued.remove(&ticket) else {
            return false;
        };
        if !delivered || !self.delivery_valid(&delivery) {
            self.rollback_delivery(&delivery);
            return false;
        }
        match delivery.kind {
            DeliveryKind::Message {
                tunnel: CLOUD_TUNNEL,
                initialize_cloud: true,
                ..
            } => {
                self.studio
                    .cloud_initialized
                    .insert(delivery.dev_id.clone());
                self.studio.connected_notifications.remove(&delivery.dev_id);
            }
            DeliveryKind::LocalConnect { .. } => self.studio.local.connected = true,
            _ => {}
        }
        true
    }

    pub(super) fn claim_delivery(&mut self, ticket: u64) -> bool {
        let Some(mut delivery) = self.studio.issued.remove(&ticket) else {
            return false;
        };
        if delivery.claimed {
            self.studio.issued.insert(ticket, delivery);
            return false;
        }
        if !self.delivery_valid(&delivery) {
            self.rollback_delivery(&delivery);
            return false;
        }
        delivery.claimed = true;
        self.studio.issued.insert(ticket, delivery);
        true
    }
}
