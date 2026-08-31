use std::ffi::c_void;

use super::ffi::{PluginStudioSnapshot, StudioSnapshotCallback};

pub(super) struct AccountFreshness {
    hub_url: String,
    token: String,
    account_epoch: u64,
    context: *mut c_void,
    snapshot: Option<StudioSnapshotCallback>,
}

impl AccountFreshness {
    pub(super) unsafe fn from_snapshot(
        raw: &PluginStudioSnapshot,
        context: *mut c_void,
        snapshot: Option<StudioSnapshotCallback>,
    ) -> Option<Self> {
        unsafe {
            if raw.account_transition_pending != 0 {
                return None;
            }
            Some(Self {
                hub_url: raw.hub_url.read("hub_url").ok()?,
                token: raw.token.read("token").ok()?,
                account_epoch: raw.account_epoch,
                context,
                snapshot,
            })
        }
    }

    pub(super) fn current(&self) -> bool {
        let Some(callback) = self.snapshot else {
            return false;
        };
        let mut current = PluginStudioSnapshot::empty();
        callback(self.context, &mut current) != 0
            && current.account_transition_pending == 0
            && current.account_epoch == self.account_epoch
            && unsafe { current.hub_url.read("hub_url") }.is_ok_and(|value| value == self.hub_url)
            && unsafe { current.token.read("token") }.is_ok_and(|value| value == self.token)
    }
}

pub(super) unsafe fn request_snapshot_current(
    expected: &PluginStudioSnapshot,
    current: &PluginStudioSnapshot,
) -> bool {
    current.account_transition_pending == 0
        && current.account_epoch == expected.account_epoch
        && current.cache_generation == expected.cache_generation
        && current.firmware_generation == expected.firmware_generation
        && unsafe { equal_bytes(current.hub_url, expected.hub_url, "hub_url") }
        && unsafe { equal_bytes(current.token, expected.token, "token") }
}

unsafe fn equal_bytes(
    left: super::ffi::PluginBytes,
    right: super::ffi::PluginBytes,
    field: &'static str,
) -> bool {
    unsafe { left.read(field) }
        .ok()
        .zip(unsafe { right.read(field) }.ok())
        .is_some_and(|(left, right)| left == right)
}
