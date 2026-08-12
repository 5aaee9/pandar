mod cache;
mod ffi;
mod http;
mod model;

#[repr(C)]
pub struct Account {
    pub hub_url: ffi::PresetBytes,
    pub token: ffi::PresetBytes,
    pub user_id: ffi::PresetBytes,
    pub account_epoch: u64,
    pub config_epoch: u64,
    pub session_kind: i32,
    pub transition_pending: i32,
    pub identity: u64,
}

impl Account {
    fn read(value: &Self) -> anyhow::Result<OwnedAccount> {
        Ok(OwnedAccount {
            hub_url: value.hub_url.read()?,
            token: value.token.read()?,
            user_id: value.user_id.read()?,
            account_epoch: value.account_epoch,
            config_epoch: value.config_epoch,
            session_kind: value.session_kind,
            transition_pending: value.transition_pending,
            identity: value.identity,
        })
    }
}

struct OwnedAccount {
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) user_id: String,
    pub(super) account_epoch: u64,
    pub(super) config_epoch: u64,
    pub(super) session_kind: i32,
    pub(super) transition_pending: i32,
    pub(super) identity: u64,
}

pub use ffi::{
    PresetBytes, PresetCallbacks, PresetEntry, PresetResult, pandar_plugin_personal_preset_drain,
    pandar_plugin_personal_preset_list, pandar_plugin_personal_preset_mutate,
    pandar_plugin_personal_preset_reset,
};
