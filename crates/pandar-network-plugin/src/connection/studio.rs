mod delivery;
mod ffi;
mod session;
mod shim_dispatch;

pub use ffi::*;
pub use shim_dispatch::{
    ShimCallbackBridge, pandar_plugin_shim_dispatch_connection_transition,
    pandar_plugin_shim_dispatch_offline_deliveries,
};

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

const DISPATCHER_IDLE_WAIT_MS: u32 = u32::MAX;
const CONNECTED_DEBOUNCE_MS: u64 = 1_000;

pub(super) const CLOUD_TUNNEL: i32 = 0;
pub(super) const LOCAL_TUNNEL: i32 = 1;
pub(super) const CLOUD_MESSAGE_LISTENER: i32 = 1;
pub(super) const LOCAL_MESSAGE_LISTENER: i32 = 2;
pub(super) const PRINTER_CONNECTED_LISTENER: i32 = 3;
pub(super) const LOCAL_CONNECTED_LISTENER: i32 = 4;

pub type StudioPayloadVisitor = extern "C" fn(
    *mut std::ffi::c_void,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
);
pub type StudioHeartbeatVisitor = extern "C" fn(*mut std::ffi::c_void, i32, *const u8, usize, u64);
pub type StudioWorkVisitor =
    extern "C" fn(*mut std::ffi::c_void, i32, i32, u64, u64, *const u8, usize, *const u8, usize);
pub type StudioRequestVisitor =
    extern "C" fn(*mut std::ffi::c_void, *const u8, usize, *const u8, usize, *const u8, usize);

#[repr(C)]
pub struct PluginStudioDeliveryResult {
    pub status: i32,
    pub ticket: u64,
    pub local_generation: u64,
    pub account_epoch: u64,
    pub cache_generation: u64,
}

#[repr(C)]
pub struct PluginStudioHeartbeatPlan {
    pub wait_ms: u32,
    pub refresh: i32,
}

#[repr(C)]
pub struct PluginStudioRequestState {
    pub status: i32,
    pub authorized: i32,
    pub account_transition_pending: i32,
    pub account_epoch: u64,
    pub cache_generation: u64,
}

#[derive(Default)]
pub(super) struct StudioState {
    selected_machine: String,
    cloud_subscriptions: BTreeSet<String>,
    cloud_initialized: BTreeSet<String>,
    connected_notifications: BTreeMap<String, u64>,
    local: LocalState,
    listeners: ListenerMask,
    pub(super) account_transition_pending: bool,
    cache_generation: u64,
    next_ticket: u64,
    issued: BTreeMap<u64, StudioDelivery>,
    pending_offline: BTreeMap<String, OfflineWork>,
    pending_connected: BTreeSet<String>,
    pending_cloud_status: BTreeMap<String, String>,
    pending_local_status: BTreeMap<String, String>,
}

#[derive(Default)]
struct LocalState {
    target: Option<String>,
    generation: u64,
    connected: bool,
}

#[derive(Default)]
struct ListenerMask {
    cloud_message: bool,
    local_message: bool,
    printer_connected: bool,
    local_connected: bool,
}

#[derive(Clone)]
pub(crate) struct StudioPayload {
    pub(crate) dev_id: String,
    pub(crate) body: String,
    pub(crate) printer_id: String,
    pub(crate) model: String,
}

pub(crate) struct StudioRequestSnapshot {
    pub(crate) hub_url: String,
    pub(crate) token: String,
    pub(crate) printer_id: String,
}

pub(crate) struct HeartbeatTarget {
    pub(super) tunnel: i32,
    pub(super) dev_id: String,
    pub(super) generation: u64,
}

pub(crate) struct StudioWork {
    pub(super) kind: i32,
    pub(super) state: i32,
    pub(super) ticket: u64,
    pub(super) generation: u64,
    pub(super) dev_id: String,
    pub(super) body: String,
}

struct OfflineWork {
    local_generation: Option<u64>,
    cloud_allowed: bool,
    forced: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeliveryKind {
    ConnectedSignal {
        notification_ms: u64,
    },
    Message {
        tunnel: i32,
        local_generation: u64,
        initialize_cloud: bool,
    },
    LocalConnect {
        generation: u64,
    },
    CloudOffline {
        forced: bool,
    },
    LocalOffline {
        generation: u64,
    },
    LocalLost {
        generation: u64,
    },
}

struct StudioDelivery {
    kind: DeliveryKind,
    dev_id: String,
    account_epoch: u64,
    printer_epoch: u64,
    cache_generation: u64,
    claimed: bool,
}

#[derive(Serialize)]
struct DisconnectEnvelope {
    event: DisconnectEvent,
}

#[derive(Serialize)]
struct DisconnectEvent {
    event: &'static str,
}

impl StudioState {
    pub(super) fn new() -> Self {
        Self {
            cache_generation: 1,
            ..Self::default()
        }
    }

    pub(super) fn reset_account(&mut self, transition_pending: bool) {
        let local = self.local.target.take().map(|dev_id| {
            self.local.generation = self.local.generation.wrapping_add(1).max(1);
            (dev_id, self.local.generation)
        });
        self.selected_machine.clear();
        self.cloud_subscriptions.clear();
        self.cloud_initialized.clear();
        self.connected_notifications.clear();
        self.local.connected = false;
        self.account_transition_pending = transition_pending;
        self.cache_generation = self.cache_generation.wrapping_add(1).max(1);
        self.issued.clear();
        self.pending_connected.clear();
        self.pending_cloud_status.clear();
        self.pending_local_status.clear();
        // Preserve the local-loss transition; final delivery rechecks its fences.
        if let Some((dev_id, generation)) = local {
            self.pending_offline.insert(
                dev_id,
                OfflineWork {
                    local_generation: Some(generation),
                    cloud_allowed: false,
                    forced: false,
                },
            );
        }
    }

    pub(super) fn finish_account_transition(&mut self) {
        self.account_transition_pending = false;
    }

    pub(super) fn invalidate_cache(&mut self) {
        self.cache_generation = self.cache_generation.wrapping_add(1).max(1);
        self.issued.clear();
    }

    pub(super) fn clear_stream_work(&mut self) {
        self.invalidate_cache();
        self.pending_offline.clear();
        self.pending_connected.clear();
        self.pending_cloud_status.clear();
        self.pending_local_status.clear();
        self.cloud_initialized.clear();
        self.connected_notifications.clear();
    }

    pub(super) fn connection_changed(&mut self) {
        self.cloud_initialized.clear();
        self.connected_notifications.clear();
        self.issued.retain(|_, delivery| {
            matches!(
                delivery.kind,
                DeliveryKind::LocalOffline { .. } | DeliveryKind::LocalLost { .. }
            )
        });
    }

    pub(super) fn queue_offline(&mut self, dev_id: String) {
        self.queue_offline_with_policy(dev_id, false);
    }

    pub(super) fn queue_forced_offline(&mut self, dev_id: String) {
        self.queue_offline_with_policy(dev_id, true);
    }

    fn queue_offline_with_policy(&mut self, dev_id: String, forced: bool) {
        self.cloud_initialized.remove(&dev_id);
        self.connected_notifications.remove(&dev_id);
        self.issued.retain(|_, delivery| delivery.dev_id != dev_id);
        let local_generation = (self.local.target.as_deref() == Some(&dev_id)).then(|| {
            self.local.target = None;
            self.local.connected = false;
            self.local.generation = self.local.generation.wrapping_add(1).max(1);
            self.local.generation
        });
        self.pending_connected.remove(&dev_id);
        self.pending_cloud_status.remove(&dev_id);
        self.pending_local_status.remove(&dev_id);
        self.pending_offline
            .entry(dev_id)
            .and_modify(|offline| {
                offline.forced |= forced;
                offline.cloud_allowed = true;
                offline.local_generation = offline.local_generation.or(local_generation);
            })
            .or_insert(OfflineWork {
                local_generation,
                cloud_allowed: true,
                forced,
            });
    }

    pub(super) fn queue_status(&mut self, dev_id: String, body: String) {
        self.queue_cloud_status(dev_id.clone(), body.clone());
        self.queue_local_status(dev_id, body);
    }

    pub(super) fn queue_cloud_status(&mut self, dev_id: String, body: String) {
        if self.listeners.cloud_message && self.cloud_target(&dev_id) {
            if self.listeners.printer_connected {
                self.pending_connected.insert(dev_id.clone());
            }
            self.pending_cloud_status.insert(dev_id, body);
        }
    }

    pub(super) fn queue_local_status(&mut self, dev_id: String, body: String) {
        if self.listeners.local_message
            && self.local.connected
            && self.local.target.as_deref() == Some(&dev_id)
        {
            self.pending_local_status.insert(dev_id, body);
        }
    }

    pub(super) fn recover_online(&mut self, dev_id: &str) {
        if self
            .pending_offline
            .get(dev_id)
            .is_none_or(|offline| !offline.forced)
        {
            self.pending_offline.remove(dev_id);
        }
        self.pending_cloud_status.remove(dev_id);
        self.pending_local_status.remove(dev_id);
    }

    fn set_listener(&mut self, kind: i32, present: bool) -> bool {
        match kind {
            CLOUD_MESSAGE_LISTENER => {
                self.listeners.cloud_message = present;
                if !present {
                    self.pending_cloud_status.clear();
                }
            }
            LOCAL_MESSAGE_LISTENER => {
                self.listeners.local_message = present;
                if !present {
                    self.pending_local_status.clear();
                }
            }
            PRINTER_CONNECTED_LISTENER => {
                self.listeners.printer_connected = present;
                if !present {
                    self.pending_connected.clear();
                }
            }
            LOCAL_CONNECTED_LISTENER => self.listeners.local_connected = present,
            _ => return false,
        }
        true
    }

    pub(super) fn cloud_target(&self, dev_id: &str) -> bool {
        self.selected_machine == dev_id || self.cloud_subscriptions.contains(dev_id)
    }

    pub(super) fn retire_cloud_target(&mut self, dev_id: &str) {
        self.cloud_initialized.remove(dev_id);
        self.connected_notifications.remove(dev_id);
        self.pending_connected.remove(dev_id);
        self.pending_cloud_status.remove(dev_id);
        self.issued.retain(|_, delivery| {
            delivery.dev_id != dev_id
                || !matches!(
                    delivery.kind,
                    DeliveryKind::ConnectedSignal { .. }
                        | DeliveryKind::Message {
                            tunnel: CLOUD_TUNNEL,
                            ..
                        }
                        | DeliveryKind::CloudOffline { .. }
                )
        });
    }

    fn heartbeat_plan(&self) -> (PluginStudioHeartbeatPlan, Vec<HeartbeatTarget>) {
        let busy = !self.pending_connected.is_empty()
            || !self.pending_cloud_status.is_empty()
            || !self.pending_local_status.is_empty()
            || !self.pending_offline.is_empty()
            || self.issued.values().any(|delivery| !delivery.claimed);
        (
            PluginStudioHeartbeatPlan {
                wait_ms: if busy {
                    crate::connection::stream::HEARTBEAT_BUSY_WAIT_MS
                } else {
                    DISPATCHER_IDLE_WAIT_MS
                },
                refresh: 0,
            },
            Vec::new(),
        )
    }
}

pub(crate) fn normalize_studio_dev_id(mut dev_id: String) -> String {
    if let Some(separator) = dev_id.find('|') {
        dev_id.truncate(separator);
    }
    dev_id
}

fn disconnect_json() -> String {
    serde_json::to_string(&DisconnectEnvelope {
        event: DisconnectEvent {
            event: "client.disconnected",
        },
    })
    .expect("disconnect event is serializable")
}
