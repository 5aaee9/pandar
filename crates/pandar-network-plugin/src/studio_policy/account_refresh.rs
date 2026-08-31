use crate::{PluginHttpResult, read_utf8, result, stable_error_body};

use super::{
    ABI_INVALID_RESULT, ACCOUNT_ACTION_APPLY, ACCOUNT_ACTION_FAILURE, ACCOUNT_ACTION_LOGIN,
    ACCOUNT_ACTION_NONE,
};

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_stale_no_auth_result() -> PluginHttpResult {
    result(
        ABI_INVALID_RESULT,
        0,
        stable_error_body("stale_no_auth_session"),
    )
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_commit_action(
    expected_epoch: u64,
    current_epoch: u64,
    expected_token_ptr: *const u8,
    expected_token_len: usize,
    current_token_ptr: *const u8,
    current_token_len: usize,
    require_logged_out: bool,
) -> i32 {
    let Some(expected_token) = (unsafe { read_utf8(expected_token_ptr, expected_token_len) })
    else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(current_token) = (unsafe { read_utf8(current_token_ptr, current_token_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let credentials_current = if require_logged_out {
        current_token.is_empty()
    } else {
        current_token == expected_token
    };
    if expected_epoch != current_epoch || !credentials_current {
        ACCOUNT_ACTION_NONE
    } else {
        ACCOUNT_ACTION_APPLY
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_refresh_action(
    expected_epoch: u64,
    current_epoch: u64,
    expected_config_epoch: u64,
    current_config_epoch: u64,
    transition_pending: bool,
    expected_session_kind: i32,
    current_session_kind: i32,
    expected_hub_ptr: *const u8,
    expected_hub_len: usize,
    current_hub_ptr: *const u8,
    current_hub_len: usize,
    expected_token_ptr: *const u8,
    expected_token_len: usize,
    current_token_ptr: *const u8,
    current_token_len: usize,
) -> i32 {
    let Some(expected_hub) = (unsafe { read_utf8(expected_hub_ptr, expected_hub_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(current_hub) = (unsafe { read_utf8(current_hub_ptr, current_hub_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(expected_token) = (unsafe { read_utf8(expected_token_ptr, expected_token_len) })
    else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(current_token) = (unsafe { read_utf8(current_token_ptr, current_token_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    if transition_pending
        || expected_session_kind != 2
        || current_session_kind != expected_session_kind
        || expected_epoch != current_epoch
        || expected_config_epoch != current_config_epoch
        || expected_hub != current_hub
        || expected_token.is_empty()
    {
        ACCOUNT_ACTION_NONE
    } else if expected_token == current_token {
        ACCOUNT_ACTION_APPLY
    } else if !current_token.is_empty() {
        ACCOUNT_ACTION_LOGIN
    } else {
        ACCOUNT_ACTION_NONE
    }
}
