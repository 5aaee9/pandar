use super::*;
use crate::connection::{AuthDisposition, ConnectionSession};

impl ConnectionSession {
    pub(crate) fn studio_camera_snapshot(&self, dev_id: String) -> Option<StudioRequestSnapshot> {
        let dev_id = normalize_studio_dev_id(dev_id);
        let state = self.state.lock().expect("connection state");
        let printer = state.printers.get(&dev_id)?;
        (state.studio_eligible(&dev_id)
            && printer.studio_local_camera
            && pandar_core::compatibility::studio_local_camera_supported(printer.model.as_deref()))
        .then(|| StudioRequestSnapshot {
            hub_url: state.hub_url.clone(),
            token: state.token.clone(),
            printer_id: printer.pandar_printer_id.clone(),
        })
    }

    pub(super) fn studio_status_target_available(
        &self,
        tunnel: i32,
        dev_id: String,
        local_generation: u64,
    ) -> bool {
        let dev_id = normalize_studio_dev_id(dev_id);
        let state = self.state.lock().expect("connection state");
        if state.studio.account_transition_pending {
            return false;
        }
        match tunnel {
            CLOUD_TUNNEL => {
                state.studio.listeners.cloud_message && state.studio.cloud_target(&dev_id)
            }
            LOCAL_TUNNEL => {
                state.studio.listeners.local_message
                    && state.studio.local.connected
                    && state.studio.local.target.as_deref() == Some(&dev_id)
                    && state.studio.local.generation == local_generation
            }
            _ => false,
        }
    }

    pub(super) fn studio_request_snapshot(
        &self,
        dev_id: String,
    ) -> (PluginStudioRequestState, StudioRequestSnapshot) {
        let dev_id = normalize_studio_dev_id(dev_id);
        let state = self.state.lock().expect("connection state");
        let printer = state.printers.get(&dev_id);
        (
            PluginStudioRequestState {
                status: 0,
                authorized: i32::from(printer.is_some()),
                account_transition_pending: i32::from(state.studio.account_transition_pending),
                account_epoch: state.account_epoch,
                cache_generation: state.studio.cache_generation,
            },
            StudioRequestSnapshot {
                hub_url: state.hub_url.clone(),
                token: state.token.clone(),
                printer_id: printer
                    .map(|printer| printer.pandar_printer_id.clone())
                    .unwrap_or(dev_id),
            },
        )
    }

    pub(super) fn studio_request_snapshot_current(
        &self,
        account_epoch: u64,
        cache_generation: u64,
    ) -> bool {
        let state = self.state.lock().expect("connection state");
        !state.studio.account_transition_pending
            && state.account_epoch == account_epoch
            && state.studio.cache_generation == cache_generation
    }

    pub(super) fn studio_account_request_admitted(&self) -> bool {
        !self
            .state
            .lock()
            .expect("connection state")
            .studio
            .account_transition_pending
    }

    pub(super) fn studio_account_request_current(&self, account_epoch: u64) -> bool {
        let state = self.state.lock().expect("connection state");
        !state.studio.account_transition_pending && state.account_epoch == account_epoch
    }

    pub(crate) fn begin_printer_cache_admission(
        &self,
        account_epoch: u64,
        require_token: bool,
        token_present: bool,
    ) -> i32 {
        let mut state = self.state.lock().expect("connection state");
        if state.studio.account_transition_pending || state.account_epoch != account_epoch {
            return 1;
        }
        if require_token && !token_present {
            return 2;
        }
        if state.printer_cache_admission_pending {
            return 1;
        }
        state.printer_cache_admission_pending = true;
        0
    }

    pub(crate) fn printer_cache_snapshot_current(&self, account_epoch: u64) -> bool {
        let state = self.state.lock().expect("connection state");
        !state.studio.account_transition_pending && state.account_epoch == account_epoch
    }

    pub(crate) fn finish_printer_cache_admission(&self) {
        self.state
            .lock()
            .expect("connection state")
            .printer_cache_admission_pending = false;
    }

    pub(super) fn begin_account_transition(&self) {
        let mut state = self.state.lock().expect("connection state");
        state.account_epoch = state.account_epoch.wrapping_add(1);
        state.generation = state.generation.wrapping_add(1);
        state.auth = AuthDisposition::Unknown;
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        state.printers_fresh = false;
        state.printers.clear();
        state.clear_deliveries();
        state.studio.reset_account(true);
    }

    pub(super) fn finish_account_transition(&self, account_epoch: u64) {
        let mut state = self.state.lock().expect("connection state");
        if state.account_epoch == account_epoch {
            state.studio.finish_account_transition();
        }
    }

    pub(super) fn studio_set_listener(&self, kind: i32, present: bool) -> bool {
        self.state
            .lock()
            .expect("connection state")
            .studio
            .set_listener(kind, present)
    }

    pub(super) fn studio_selected(&self) -> String {
        self.state
            .lock()
            .expect("connection state")
            .studio
            .selected_machine
            .clone()
    }

    pub(super) fn studio_set_selected(&self, selected: String) {
        let selected = normalize_studio_dev_id(selected);
        let mut state = self.state.lock().expect("connection state");
        let previous = std::mem::replace(&mut state.studio.selected_machine, selected.clone());
        if previous != selected
            && !previous.is_empty()
            && !state.studio.cloud_subscriptions.contains(&previous)
        {
            state.studio.retire_cloud_target(&previous);
        }
    }

    pub(super) fn studio_add_subscription(&self, dev_id: String) {
        self.state
            .lock()
            .expect("connection state")
            .studio
            .cloud_subscriptions
            .insert(normalize_studio_dev_id(dev_id));
    }

    pub(super) fn studio_del_subscription(&self, dev_id: String) {
        let dev_id = normalize_studio_dev_id(dev_id);
        let mut state = self.state.lock().expect("connection state");
        state.studio.cloud_subscriptions.remove(&dev_id);
        if !state.studio.cloud_target(&dev_id) {
            state.studio.retire_cloud_target(&dev_id);
        }
    }

    pub(super) fn studio_heartbeat_plan(
        &self,
    ) -> (PluginStudioHeartbeatPlan, Vec<HeartbeatTarget>) {
        self.state
            .lock()
            .expect("connection state")
            .studio
            .heartbeat_plan()
    }

    pub(super) fn studio_prepare_connected(
        &self,
        dev_id: String,
        now_ms: u64,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        self.state
            .lock()
            .expect("connection state")
            .prepare_connected(normalize_studio_dev_id(dev_id), now_ms)
    }

    pub(super) fn studio_prepare_message(
        &self,
        tunnel: i32,
        dev_id: String,
        local_generation: u64,
        initialize_cloud: bool,
        expected_cache_generation: u64,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        self.state
            .lock()
            .expect("connection state")
            .prepare_message(
                tunnel,
                normalize_studio_dev_id(dev_id),
                local_generation,
                initialize_cloud,
                expected_cache_generation,
            )
    }

    pub(super) fn studio_connect_local(
        &self,
        dev_id: String,
    ) -> (PluginStudioDeliveryResult, Option<StudioPayload>) {
        self.state
            .lock()
            .expect("connection state")
            .connect_local(normalize_studio_dev_id(dev_id))
    }

    pub(super) fn studio_disconnect_local(&self) {
        let mut state = self.state.lock().expect("connection state");
        state.studio.local.target = None;
        state.studio.local.connected = false;
        state.studio.local.generation = state.studio.local.generation.wrapping_add(1).max(1);
        state.studio.issued.retain(|_, delivery| {
            !matches!(
                delivery.kind,
                DeliveryKind::Message {
                    tunnel: LOCAL_TUNNEL,
                    ..
                } | DeliveryKind::LocalConnect { .. }
                    | DeliveryKind::LocalOffline { .. }
                    | DeliveryKind::LocalLost { .. }
            )
        });
    }

    pub(super) fn studio_local_generation(&self, dev_id: String) -> u64 {
        let dev_id = normalize_studio_dev_id(dev_id);
        let state = self.state.lock().expect("connection state");
        if state.studio_eligible(&dev_id)
            && state.studio.local.connected
            && state.studio.local.target.as_deref() == Some(&dev_id)
        {
            state.studio.local.generation
        } else {
            0
        }
    }

    pub(super) fn studio_complete_delivery(&self, ticket: u64, delivered: bool) -> bool {
        self.state
            .lock()
            .expect("connection state")
            .complete_delivery(ticket, delivered)
    }

    pub(super) fn studio_claim_delivery(&self, ticket: u64) -> bool {
        self.state
            .lock()
            .expect("connection state")
            .claim_delivery(ticket)
    }

    pub(super) fn studio_take_work(&self) -> Vec<StudioWork> {
        self.state.lock().expect("connection state").take_work()
    }
}
