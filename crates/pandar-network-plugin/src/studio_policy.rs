use crate::{PluginHttpResult, read_utf8, result, stable_error_body};

pub(crate) mod account_refresh;
pub(crate) mod login_observation;
mod results;

pub(crate) use results::*;

#[cfg(test)]
use account_refresh::{pandar_plugin_account_commit_action, pandar_plugin_account_refresh_action};

const ABI_INVALID_HANDLE: i32 = -1;
const ABI_CONNECT_FAILED: i32 = -2;
const ABI_GET_USER_PRINT_INFO_FAILED: i32 = -11;
const ABI_INVALID_RESULT: i32 = -19;

pub(crate) const ACCOUNT_ACTION_FAILURE: i32 = -1;
pub(crate) const ACCOUNT_ACTION_NONE: i32 = 0;
pub(crate) const ACCOUNT_ACTION_APPLY: i32 = 1;
pub(crate) const ACCOUNT_ACTION_LOGOUT: i32 = 3;
pub(crate) const ACCOUNT_ACTION_LOGIN: i32 = 4;

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_response_action(status: i32) -> i32 {
    if status == 0 {
        ACCOUNT_ACTION_APPLY
    } else {
        ACCOUNT_ACTION_FAILURE
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_response_status(status: i32) -> i32 {
    if status == 0 { 0 } else { ABI_INVALID_RESULT }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_is_logged_in(
    token_ptr: *const u8,
    token_len: usize,
) -> bool {
    unsafe { read_utf8(token_ptr, token_len) }.is_some_and(|token| !token.is_empty())
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_studio_info_url(
    agent_valid: bool,
    configured: bool,
    url_ptr: *const u8,
    url_len: usize,
) -> PluginHttpResult {
    if !agent_valid || !configured {
        return crate::studio_disposition::pandar_plugin_studio_disposition(53, agent_valid);
    }
    let Some(url) = unsafe { read_utf8(url_ptr, url_len) }.filter(|url| !url.is_empty()) else {
        return crate::studio_disposition::pandar_plugin_studio_disposition(53, true);
    };
    result(0, 200, url)
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_print_info_admission(
    agent_valid: bool,
    account_transition_pending: bool,
    token_ptr: *const u8,
    token_len: usize,
) -> PluginHttpResult {
    if !agent_valid {
        return result(ABI_INVALID_HANDLE, 0, stable_error_body("invalid_handle"));
    }
    if account_transition_pending {
        return result(
            ABI_GET_USER_PRINT_INFO_FAILED,
            409,
            stable_error_body("account_transition"),
        );
    }
    if !unsafe { pandar_plugin_account_is_logged_in(token_ptr, token_len) } {
        return result(
            ABI_GET_USER_PRINT_INFO_FAILED,
            401,
            stable_error_body("invalid_auth_token"),
        );
    }
    result(0, 0, "")
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_request_admitted(
    printer_authorized: bool,
    account_transition_pending: bool,
) -> PluginHttpResult {
    studio_request_admitted(printer_authorized, account_transition_pending)
}

pub(crate) fn studio_request_admitted(
    printer_authorized: bool,
    account_transition_pending: bool,
) -> PluginHttpResult {
    if account_transition_pending {
        return result(
            ABI_INVALID_RESULT,
            409,
            stable_error_body("account_transition"),
        );
    }
    if !printer_authorized {
        return result(
            ABI_INVALID_RESULT,
            404,
            stable_error_body("invalid_printer_id"),
        );
    }
    result(0, 0, "")
}

#[cfg(test)]
mod tests;
