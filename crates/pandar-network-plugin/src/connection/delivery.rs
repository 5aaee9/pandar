use std::collections::BTreeMap;

use super::{
    AuthDisposition, ConnectionSession, ConnectionState, PluginConnectionResult, Reachability,
    connection_result_from_state,
};

#[derive(Clone, PartialEq, Eq)]
enum IssuedDelivery {
    Reachability(Reachability),
    AuthRejected,
    PrinterOffline(String),
}

#[derive(Default)]
pub(super) struct DeliveryState {
    next_ticket: u64,
    issued: BTreeMap<u64, IssuedDelivery>,
}

impl DeliveryState {
    fn issue(&mut self, delivery: IssuedDelivery) -> u64 {
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        self.issued.insert(self.next_ticket, delivery);
        self.next_ticket
    }

    fn claim(&mut self, ticket: u64) -> bool {
        ticket != 0 && self.issued.remove(&ticket).is_some()
    }

    fn clear(&mut self) {
        self.issued.clear();
    }

    fn invalidate_reachability(&mut self) {
        self.issued
            .retain(|_, delivery| !matches!(delivery, IssuedDelivery::Reachability(_)));
    }

    fn invalidate_auth(&mut self) {
        self.issued
            .retain(|_, delivery| *delivery != IssuedDelivery::AuthRejected);
    }

    fn invalidate_offline(&mut self, dev_id: &str) {
        self.issued.retain(|_, delivery| {
            !matches!(delivery, IssuedDelivery::PrinterOffline(issued) if issued == dev_id)
        });
    }

    fn has_offline(&self, dev_id: &str) -> bool {
        self.issued.values().any(
            |delivery| matches!(delivery, IssuedDelivery::PrinterOffline(issued) if issued == dev_id),
        )
    }
}

pub(crate) struct IssuedOffline {
    pub(crate) dev_id: String,
    pub(crate) ticket: u64,
}

impl ConnectionState {
    pub(super) fn clear_deliveries(&mut self) {
        self.pending_transition = None;
        self.pending_auth_transition = false;
        self.pending_offline.clear();
        self.degraded_offline.clear();
        self.unconfirmed_online.clear();
        self.delivery.clear();
    }

    pub(super) fn reset_auth_delivery(&mut self) {
        self.auth = AuthDisposition::Unknown;
        self.pending_auth_transition = false;
        self.delivery.invalidate_auth();
    }

    pub(super) fn capture_online(&mut self) {
        if !self.printers_fresh {
            return;
        }
        self.unconfirmed_online.extend(
            self.printers
                .values()
                .filter(|printer| printer.online)
                .map(|printer| printer.dev_id.clone()),
        );
    }

    pub(super) fn set_reachability(&mut self, next: Reachability) -> bool {
        if self.reachability == next {
            return false;
        }
        self.delivery.invalidate_reachability();
        self.reachability = next;
        self.pending_transition = Some(next);
        self.studio.connection_changed();
        true
    }

    pub(super) fn reject_auth(&mut self) {
        if self.auth != AuthDisposition::Rejected {
            self.delivery.invalidate_auth();
            self.auth = AuthDisposition::Rejected;
            self.pending_auth_transition = true;
        }
    }

    pub(super) fn accept_auth(&mut self) {
        self.delivery.invalidate_auth();
        self.auth = AuthDisposition::Accepted;
        self.pending_auth_transition = false;
    }

    pub(super) fn queue_offline(&mut self, dev_ids: impl IntoIterator<Item = String>) {
        for dev_id in dev_ids {
            if !self.pending_offline.contains(&dev_id) && !self.delivery.has_offline(&dev_id) {
                self.pending_offline.push(dev_id.clone());
                self.studio.queue_offline(dev_id);
            }
        }
    }

    pub(super) fn queue_forced_offline(&mut self, dev_ids: impl IntoIterator<Item = String>) {
        for dev_id in dev_ids {
            self.degraded_offline.insert(dev_id.clone());
            if !self.pending_offline.contains(&dev_id) && !self.delivery.has_offline(&dev_id) {
                self.pending_offline.push(dev_id.clone());
            }
            self.studio.queue_forced_offline(dev_id);
        }
    }

    pub(super) fn recover_online(&mut self, dev_id: &str) {
        if !self.degraded_offline.contains(dev_id) {
            self.pending_offline.retain(|pending| pending != dev_id);
            self.delivery.invalidate_offline(dev_id);
        }
        self.studio.recover_online(dev_id);
    }
}

impl ConnectionSession {
    pub(crate) fn take_transition(&self) -> PluginConnectionResult {
        let mut state = self.state.lock().expect("connection state");
        let transition = state.pending_transition.take();
        let auth_changed = state.pending_auth_transition;
        state.pending_auth_transition = false;
        let transition_ticket = transition
            .map(|reachability| {
                state
                    .delivery
                    .issue(IssuedDelivery::Reachability(reachability))
            })
            .unwrap_or_default();
        let auth_ticket = if auth_changed {
            state.delivery.issue(IssuedDelivery::AuthRejected)
        } else {
            0
        };
        let mut result =
            connection_result_from_state(&state, 0, transition_ticket != 0, auth_ticket != 0);
        result.transition_ticket = transition_ticket;
        result.auth_ticket = auth_ticket;
        result
    }

    pub(crate) fn take_offline(&self) -> Vec<IssuedOffline> {
        let mut state = self.state.lock().expect("connection state");
        std::mem::take(&mut state.pending_offline)
            .into_iter()
            .map(|dev_id| {
                state.degraded_offline.remove(&dev_id);
                IssuedOffline {
                    ticket: state
                        .delivery
                        .issue(IssuedDelivery::PrinterOffline(dev_id.clone())),
                    dev_id,
                }
            })
            .collect()
    }

    pub(super) fn claim_delivery(&self, ticket: u64) -> bool {
        self.state
            .lock()
            .expect("connection state")
            .delivery
            .claim(ticket)
    }

    pub(in crate::connection) fn retry_connection_callback(&self, result: &PluginConnectionResult) {
        let mut state = self.state.lock().expect("connection state");
        if result.changed != 0 {
            let expected = if result.connected != 0 {
                Reachability::Connected
            } else {
                Reachability::Disconnected
            };
            if state.reachability == expected {
                state.pending_transition = Some(expected);
            }
        }
        if result.auth_changed != 0 && state.auth == AuthDisposition::Rejected {
            state.pending_auth_transition = true;
        }
    }
}
