mod account_logout;
mod delivery;
pub(crate) mod ffi;
pub(crate) mod no_auth;
mod no_auth_refresh;
pub(crate) mod no_auth_rotation;
mod printers;
mod request;
mod studio;
mod types;

pub use ffi::*;
pub use no_auth_refresh::*;
pub use studio::*;
pub use types::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    sync::{Condvar, Mutex},
};

use serde::Deserialize;

use crate::{
    PluginHttpResult, RequestKind, http, invalid_input, normalize_hub_url, read_utf8, result,
    stable_error_body,
    studio_status::{PrinterObservation, project_hub_printers},
};
pub(crate) use account_logout::AccountLogoutBegin;
use account_logout::AccountLogoutCoordinator;
use delivery::DeliveryState;
use request::{RequestSnapshot, fetch_printers, fetch_readiness};
use studio::StudioState;

pub(crate) struct ConnectionSession {
    state: Mutex<ConnectionState>,
    account_logout: AccountLogoutCoordinator,
    no_auth_rotation_changed: Condvar,
    readiness_request: Mutex<()>,
    printer_request: Mutex<()>,
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

pub(super) struct ConnectionState {
    hub_url: String,
    token: String,
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
    delivery: DeliveryState,
    no_auth_retry: no_auth::NoAuthRetry,
    no_auth_rotation: no_auth_rotation::NoAuthRotation,
    studio: StudioState,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

impl ConnectionSession {
    pub(crate) fn new(hub_url: String, token: String) -> Self {
        Self {
            account_logout: AccountLogoutCoordinator::new(),
            state: Mutex::new(ConnectionState {
                hub_url,
                token,
                generation: 0,
                account_epoch: 0,
                reachability: Reachability::Unknown,
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
            }),
            no_auth_rotation_changed: Condvar::new(),
            readiness_request: Mutex::new(()),
            printer_request: Mutex::new(()),
        }
    }

    fn update(&self, hub_url: String, token: String) {
        let mut state = self.state.lock().expect("connection state");
        if state.hub_url == hub_url && state.token == token {
            return;
        }
        let hub_changed = state.hub_url != hub_url;
        if !hub_changed {
            state.capture_online();
        }
        state.hub_url = hub_url;
        state.token = token;
        state.generation = state.generation.wrapping_add(1);
        if hub_changed {
            state.reachability = Reachability::Unknown;
            state.auth = AuthDisposition::Unknown;
            state.printers.clear();
            state.clear_deliveries();
            state.studio.reset_account(false);
        } else {
            state.reset_auth_delivery();
            state.studio.invalidate_cache();
        }
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        state.printers_fresh = false;
        drop(state);
        self.no_auth_rotation_changed.notify_all();
    }

    fn snapshot(&self) -> RequestSnapshot {
        let state = self.state.lock().expect("connection state");
        RequestSnapshot {
            hub_url: state.hub_url.clone(),
            token: state.token.clone(),
            generation: state.generation,
            printer_epoch: state.printer_epoch,
        }
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
        state.printers.clear();
        state.clear_deliveries();
        state.studio.reset_account(false);
        drop(state);
        self.no_auth_rotation_changed.notify_all();
    }

    fn is_connected(&self) -> bool {
        self.state.lock().expect("connection state").reachability == Reachability::Connected
    }

    fn refresh_connection(&self) -> PluginConnectionResult {
        let Ok(_request) = self.readiness_request.try_lock() else {
            return self.connection_result(0, false);
        };
        let snapshot = self.snapshot();
        let observation = fetch_readiness(&snapshot);
        let (http_code, connected) = match observation {
            Ok(response) => {
                let ready = if response.http_code == 200 {
                    match serde_json::from_str::<HealthResponse>(&response.body) {
                        Ok(body) => body.status == "ok",
                        Err(error) => {
                            eprintln!(
                                "pandar Hub readiness refresh failed: {:#}",
                                anyhow::Error::new(error)
                                    .context("validate Hub readiness response")
                            );
                            false
                        }
                    }
                } else {
                    false
                };
                (response.http_code, ready)
            }
            Err(error) => {
                eprintln!("pandar Hub readiness refresh failed: {error:#}");
                (0, false)
            }
        };
        let next = if connected {
            Reachability::Connected
        } else {
            Reachability::Disconnected
        };
        let mut state = self.state.lock().expect("connection state");
        if state.generation != snapshot.generation {
            return connection_result_from_state(&state, 0, false, false);
        }
        let changed = state.set_reachability(next);
        connection_result_from_state(&state, http_code, changed, false)
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
}

impl RequestSnapshot {
    fn is_current(&self, state: &ConnectionState) -> bool {
        state.generation == self.generation && state.printer_epoch == self.printer_epoch
    }
}

fn connection_result_from_state(
    state: &ConnectionState,
    http_code: u32,
    changed: bool,
    auth_changed: bool,
) -> PluginConnectionResult {
    let connected = state.reachability == Reachability::Connected;
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
