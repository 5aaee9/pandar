mod account_logout;
mod delivery;
pub(crate) mod ffi;
pub(crate) mod no_auth;
pub(crate) mod no_auth_refresh;
pub(crate) mod no_auth_rotation;
pub(crate) mod request;
pub(crate) mod stream;
mod studio;
mod types;

pub use ffi::*;
pub use no_auth_refresh::*;
pub use studio::*;
pub use types::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use crate::{
    PluginHttpResult, invalid_input, normalize_hub_url, read_utf8, result, stable_error_body,
    studio_status::{FirmwareObservation, FirmwareProjection, PrinterObservation},
};
pub(crate) use account_logout::AccountLogoutBegin;
use account_logout::AccountLogoutCoordinator;
use delivery::DeliveryState;
use request::RequestSnapshot;
use stream::{StreamSignals, StreamWorker};
use studio::StudioState;

pub(crate) struct ConnectionSession {
    state: Arc<Mutex<ConnectionState>>,
    account_logout: AccountLogoutCoordinator,
    no_auth_rotation_changed: Condvar,
    signals: Arc<StreamSignals>,
    dispatcher: Arc<Mutex<Option<DispatcherWake>>>,
    worker: Mutex<Option<StreamWorker>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reachability {
    Unknown,
    Connected,
    Disconnected,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthDisposition {
    Unknown,
    Accepted,
    Rejected,
}

#[derive(Clone, Copy)]
pub(super) struct DispatcherWake {
    context: usize,
    callback: extern "C" fn(*mut c_void),
}

impl DispatcherWake {
    fn notify(self) {
        (self.callback)(self.context as *mut c_void);
    }
}

pub(super) struct ConnectionState {
    hub_url: String,
    token: String,
    tenant_id: String,
    generation: u64,
    account_epoch: u64,
    reachability: Reachability,
    pending_transition: Option<Reachability>,
    auth: AuthDisposition,
    pending_auth_transition: bool,
    printer_epoch: u64,
    printers_fresh: bool,
    printers: BTreeMap<String, PrinterObservation>,
    unconfirmed_online: BTreeSet<String>,
    pending_offline: Vec<String>,
    printer_cache_admission_pending: bool,
    stream_degraded: bool,
    stream_error_pending: bool,
    degraded_offline: BTreeSet<String>,
    delivery: DeliveryState,
    no_auth_retry: no_auth::NoAuthRetry,
    no_auth_rotation: no_auth_rotation::NoAuthRotation,
    studio: StudioState,
}

impl ConnectionState {
    pub(super) fn stream_config(&self) -> Option<stream::StreamConfig> {
        let hub_url = normalize_hub_url(self.hub_url.clone())?;
        if self.studio.account_transition_pending || self.tenant_id.trim().is_empty() {
            return None;
        }
        Some(stream::StreamConfig {
            url: stream::printer_events_url(&hub_url, &self.tenant_id)?,
            hub_url,
            token: self.token.clone(),
            generation: self.generation,
            account_epoch: self.account_epoch,
        })
    }
}

impl ConnectionSession {
    pub(crate) fn new(hub_url: String, token: String) -> Self {
        Self {
            account_logout: AccountLogoutCoordinator::new(),
            state: Arc::new(Mutex::new(ConnectionState {
                hub_url,
                token,
                tenant_id: String::new(),
                generation: 0,
                account_epoch: 0,
                reachability: Reachability::Unknown,
                stream_degraded: false,
                stream_error_pending: false,
                degraded_offline: BTreeSet::new(),
                pending_transition: None,
                auth: AuthDisposition::Unknown,
                pending_auth_transition: false,
                printer_epoch: 0,
                printers_fresh: false,
                printers: BTreeMap::new(),
                unconfirmed_online: BTreeSet::new(),
                pending_offline: Vec::new(),
                printer_cache_admission_pending: false,
                delivery: DeliveryState::default(),
                no_auth_retry: no_auth::NoAuthRetry::default(),
                no_auth_rotation: no_auth_rotation::NoAuthRotation::default(),
                studio: StudioState::new(),
            })),
            no_auth_rotation_changed: Condvar::new(),
            signals: Arc::new(StreamSignals::new()),
            dispatcher: Arc::new(Mutex::new(None)),
            worker: Mutex::new(None),
        }
    }

    fn update(&self, hub_url: String, token: String) {
        let mut state = self.state.lock().expect("connection state");
        if state.hub_url == hub_url && state.token == token {
            return;
        }
        if state.hub_url != hub_url {
            let transition_pending = state.studio.account_transition_pending;
            state.hub_url = hub_url;
            state.reachability = Reachability::Unknown;
            state.auth = AuthDisposition::Unknown;
            state.printers.clear();
            state.clear_deliveries();
            state.studio.reset_account(transition_pending);
        } else {
            let transition_pending = state.studio.account_transition_pending;
            state.capture_online();
            state.clear_deliveries();
            state.reset_auth_delivery();
            if transition_pending {
                state.studio.invalidate_cache();
            } else {
                state.studio.clear_stream_work();
            }
        }
        state.token = token;
        state.generation = state.generation.wrapping_add(1);
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        state.printers_fresh = false;
        state.stream_degraded = false;
        state.stream_error_pending = false;
        drop(state);
        self.no_auth_rotation_changed.notify_all();
        self.wake_worker();
        self.notify_dispatcher();
    }

    fn set_account_epoch(&self, account_epoch: u64) {
        let mut state = self.state.lock().expect("connection state");
        if state.account_epoch == account_epoch {
            return;
        }
        state.account_epoch = account_epoch;
        state.generation = state.generation.wrapping_add(1);
        state.auth = AuthDisposition::Unknown;
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        state.printers_fresh = false;
        state.stream_degraded = false;
        state.stream_error_pending = false;
        state.printers.clear();
        state.clear_deliveries();
        let transition_pending = state.studio.account_transition_pending;
        state.studio.reset_account(transition_pending);
        drop(state);
        self.no_auth_rotation_changed.notify_all();
        self.wake_worker();
        self.notify_dispatcher();
    }

    pub(super) fn set_tenant(&self, tenant_id: String) {
        let mut state = self.state.lock().expect("connection state");
        if state.tenant_id == tenant_id {
            return;
        }
        state.tenant_id = tenant_id;
        state.generation = state.generation.wrapping_add(1);
        state.auth = AuthDisposition::Unknown;
        state.reachability = Reachability::Unknown;
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        state.printers_fresh = false;
        state.stream_degraded = false;
        state.stream_error_pending = false;
        state.printers.clear();
        state.clear_deliveries();
        let transition_pending = state.studio.account_transition_pending;
        state.studio.reset_account(transition_pending);
        drop(state);
        self.no_auth_rotation_changed.notify_all();
        self.wake_worker();
        self.notify_dispatcher();
    }

    fn is_connected(&self) -> bool {
        let state = self.state.lock().expect("connection state");
        state.reachability == Reachability::Connected
            && state.auth == AuthDisposition::Accepted
            && state.printers_fresh
    }

    fn refresh_connection(&self) -> PluginConnectionResult {
        self.wake_worker();
        self.connection_result(0, false)
    }

    fn connection_result(&self, http_code: u32, changed: bool) -> PluginConnectionResult {
        connection_result_from_state(
            &self.state.lock().expect("connection state"),
            http_code,
            changed,
            false,
        )
    }

    fn printer_eligible(&self, dev_id: &str) -> bool {
        let state = self.state.lock().expect("connection state");
        !state.printer_cache_admission_pending
            && state.reachability == Reachability::Connected
            && state.auth != AuthDisposition::Rejected
            && state.printers_fresh
            && state
                .printers
                .get(dev_id)
                .is_some_and(|printer| printer.online)
    }

    fn fresh_printers(&self) -> Option<Vec<PrinterObservation>> {
        let state = self.state.lock().expect("connection state");
        state
            .printers_fresh
            .then(|| state.printers.values().cloned().collect())
    }

    fn cached_print_info(&self) -> Option<String> {
        let state = self.state.lock().expect("connection state");
        state
            .printers_fresh
            .then(|| print_devices_envelope(&state.printers))
    }

    pub(super) fn wait_cached_print_info(
        &self,
        account_epoch: u64,
        timeout: Duration,
    ) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.wake_worker();
            {
                let state = self.state.lock().expect("connection state");
                if state.printers_fresh && state.account_epoch == account_epoch {
                    return Some(print_devices_envelope(&state.printers));
                }
                // A rejected stream can never commit a snapshot; waiting
                // would only burn the caller's budget.
                if state.auth == AuthDisposition::Rejected {
                    return None;
                }
                state.stream_config()?;
            }
            if Instant::now() >= deadline {
                return None;
            }
            self.signals.wait_for_snapshot(Duration::from_millis(100));
        }
    }

    pub(super) fn cached_firmware_projection(&self) -> Option<FirmwareProjection> {
        let state = self.state.lock().expect("connection state");
        if !state.printers_fresh {
            return None;
        }
        let observations = state
            .printers
            .values()
            .map(|printer| FirmwareObservation {
                dev_id: printer.dev_id.clone(),
                firmware: printer.firmware.clone(),
            })
            .collect();
        Some(FirmwareProjection::from_observations(
            state.printers.len(),
            observations,
        ))
    }

    pub(super) fn take_stream_error(&self) -> PluginHttpResult {
        let mut state = self.state.lock().expect("connection state");
        if !state.stream_error_pending {
            return result(0, 0, String::new());
        }
        state.stream_error_pending = false;
        result(
            1,
            503,
            stable_error_body("printer_event_stream_unavailable"),
        )
    }

    pub(super) fn set_dispatcher_waker(
        &self,
        context: *mut c_void,
        callback: Option<extern "C" fn(*mut c_void)>,
    ) {
        *self.dispatcher.lock().expect("dispatcher waker") =
            callback.map(|callback| DispatcherWake {
                context: context as usize,
                callback,
            });
    }

    pub(super) fn notify_dispatcher(&self) {
        if let Some(wake) = *self.dispatcher.lock().expect("dispatcher waker") {
            wake.notify();
        }
    }

    pub(super) fn wake_worker(&self) {
        let mut worker = self.worker.lock().expect("stream worker handle");
        match worker.as_mut() {
            Some(worker) => worker.wake(),
            None => {
                *worker = Some(StreamWorker::spawn(
                    Arc::clone(&self.state),
                    Arc::clone(&self.signals),
                    Arc::clone(&self.dispatcher),
                ));
            }
        }
    }

    pub(super) fn stop_worker(&self) {
        if let Some(worker) = self.worker.lock().expect("stream worker handle").take() {
            worker.cancel_and_join();
        }
    }
}

fn print_devices_envelope(printers: &BTreeMap<String, PrinterObservation>) -> String {
    let devices = printers
        .values()
        .map(|printer| printer.raw_device.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"message\":\"success\",\"devices\":[{devices}]}}")
}

fn connection_result_from_state(
    state: &ConnectionState,
    http_code: u32,
    changed: bool,
    auth_changed: bool,
) -> PluginConnectionResult {
    let connected = state.reachability == Reachability::Connected
        && state.auth == AuthDisposition::Accepted
        && state.printers_fresh;
    PluginConnectionResult {
        status: i32::from(!connected),
        http_code,
        connected: i32::from(connected),
        changed: i32::from(changed),
        auth_rejected: i32::from(state.auth == AuthDisposition::Rejected),
        auth_changed: i32::from(auth_changed),
        transition_ticket: 0,
        auth_ticket: 0,
    }
}
