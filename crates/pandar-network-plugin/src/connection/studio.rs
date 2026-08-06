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

const HEARTBEAT_INTERVAL_MS: u32 = 2_000;
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
    account_transition_pending: bool,
    cache_generation: u64,
    next_ticket: u64,
    issued: BTreeMap<u64, StudioDelivery>,
    pending_offline: BTreeMap<String, OfflineWork>,
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
pub(super) struct StudioPayload {
    pub(super) dev_id: String,
    pub(super) body: String,
    pub(super) printer_id: String,
    pub(super) model: String,
}

pub(crate) struct StudioRequestSnapshot {
    pub(crate) hub_url: String,
    pub(crate) token: String,
    pub(crate) printer_id: String,
}

pub(super) struct HeartbeatTarget {
    pub(super) tunnel: i32,
    pub(super) dev_id: String,
    pub(super) generation: u64,
}

pub(super) struct StudioWork {
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
    CloudOffline,
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
        self.pending_offline.clear();
        if let Some((dev_id, generation)) = local {
            self.pending_offline.insert(
                dev_id,
                OfflineWork {
                    local_generation: Some(generation),
                    cloud_allowed: false,
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
        self.cloud_initialized.remove(&dev_id);
        self.connected_notifications.remove(&dev_id);
        self.issued.retain(|_, delivery| delivery.dev_id != dev_id);
        let local_generation = (self.local.target.as_deref() == Some(&dev_id)).then(|| {
            self.local.target = None;
            self.local.connected = false;
            self.local.generation = self.local.generation.wrapping_add(1).max(1);
            self.local.generation
        });
        self.pending_offline.insert(
            dev_id,
            OfflineWork {
                local_generation,
                cloud_allowed: true,
            },
        );
    }

    pub(super) fn recover_online(&mut self, dev_id: &str) {
        self.pending_offline.remove(dev_id);
    }

    fn set_listener(&mut self, kind: i32, present: bool) -> bool {
        match kind {
            CLOUD_MESSAGE_LISTENER => self.listeners.cloud_message = present,
            LOCAL_MESSAGE_LISTENER => self.listeners.local_message = present,
            PRINTER_CONNECTED_LISTENER => self.listeners.printer_connected = present,
            LOCAL_CONNECTED_LISTENER => self.listeners.local_connected = present,
            _ => return false,
        }
        true
    }

    fn cloud_target(&self, dev_id: &str) -> bool {
        self.selected_machine == dev_id || self.cloud_subscriptions.contains(dev_id)
    }

    fn retire_cloud_target(&mut self, dev_id: &str) {
        self.cloud_initialized.remove(dev_id);
        self.connected_notifications.remove(dev_id);
        self.issued.retain(|_, delivery| {
            delivery.dev_id != dev_id
                || !matches!(
                    delivery.kind,
                    DeliveryKind::ConnectedSignal { .. }
                        | DeliveryKind::Message {
                            tunnel: CLOUD_TUNNEL,
                            ..
                        }
                        | DeliveryKind::CloudOffline
                )
        });
    }

    fn heartbeat_plan(&self) -> (PluginStudioHeartbeatPlan, Vec<HeartbeatTarget>) {
        let mut targets = Vec::new();
        if self.listeners.cloud_message {
            let mut cloud_targets = self.cloud_subscriptions.clone();
            if !self.selected_machine.is_empty() {
                cloud_targets.insert(self.selected_machine.clone());
            }
            targets.extend(cloud_targets.into_iter().map(|dev_id| HeartbeatTarget {
                tunnel: CLOUD_TUNNEL,
                dev_id,
                generation: 0,
            }));
        }
        if self.listeners.local_message
            && let Some(dev_id) = self.local.target.clone().filter(|_| self.local.connected)
        {
            targets.push(HeartbeatTarget {
                tunnel: LOCAL_TUNNEL,
                dev_id,
                generation: self.local.generation,
            });
        }
        (
            PluginStudioHeartbeatPlan {
                wait_ms: HEARTBEAT_INTERVAL_MS,
                refresh: i32::from(!targets.is_empty()),
            },
            targets,
        )
    }
}

pub(super) fn normalize_studio_dev_id(mut dev_id: String) -> String {
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
