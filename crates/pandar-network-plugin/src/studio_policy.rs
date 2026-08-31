use crate::{PluginHttpResult, read_utf8, result, stable_error_body};

pub(crate) mod account_refresh;
pub(crate) mod login_observation;
mod results;

pub(crate) use results::*;

#[cfg(test)]
use account_refresh::{
    pandar_plugin_account_commit_action, pandar_plugin_account_refresh_action,
    pandar_plugin_account_stale_no_auth_result,
};

const ABI_INVALID_HANDLE: i32 = -1;
const ABI_CONNECT_FAILED: i32 = -2;
const ABI_GET_USER_PRINT_INFO_FAILED: i32 = -11;
const ABI_INVALID_RESULT: i32 = -19;

pub(crate) const ACCOUNT_ACTION_FAILURE: i32 = -1;
pub(crate) const ACCOUNT_ACTION_NONE: i32 = 0;
pub(crate) const ACCOUNT_ACTION_APPLY: i32 = 1;
const ACCOUNT_ACTION_RESET: i32 = 2;
pub(crate) const ACCOUNT_ACTION_LOGOUT: i32 = 3;
pub(crate) const ACCOUNT_ACTION_LOGIN: i32 = 4;

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_hub_action(
    current_ptr: *const u8,
    current_len: usize,
    replacement_ptr: *const u8,
    replacement_len: usize,
) -> i32 {
    let Some(current) = (unsafe { read_utf8(current_ptr, current_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(replacement) = (unsafe { read_utf8(replacement_ptr, replacement_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    if replacement.is_empty() {
        ACCOUNT_ACTION_FAILURE
    } else if current == replacement {
        ACCOUNT_ACTION_NONE
    } else {
        ACCOUNT_ACTION_RESET
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_load_action(
    load_status: i32,
    current_token_ptr: *const u8,
    current_token_len: usize,
    current_hub_ptr: *const u8,
    current_hub_len: usize,
    expected_hub_ptr: *const u8,
    expected_hub_len: usize,
) -> i32 {
    if load_status == 2 {
        return ACCOUNT_ACTION_NONE;
    }
    if load_status != 0 {
        return ACCOUNT_ACTION_FAILURE;
    }
    let Some(current_token) = (unsafe { read_utf8(current_token_ptr, current_token_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(current_hub) = (unsafe { read_utf8(current_hub_ptr, current_hub_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    let Some(expected_hub) = (unsafe { read_utf8(expected_hub_ptr, expected_hub_len) }) else {
        return ACCOUNT_ACTION_FAILURE;
    };
    if current_token.is_empty() && current_hub == expected_hub {
        ACCOUNT_ACTION_APPLY
    } else {
        ACCOUNT_ACTION_NONE
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_bootstrap_action(
    token_ptr: *const u8,
    token_len: usize,
) -> i32 {
    match unsafe { read_utf8(token_ptr, token_len) } {
        Some(token) if token.is_empty() => ACCOUNT_ACTION_APPLY,
        Some(_) => ACCOUNT_ACTION_NONE,
        None => ACCOUNT_ACTION_FAILURE,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_change_action(
    user_info_ptr: *const u8,
    user_info_len: usize,
) -> i32 {
    match unsafe { read_utf8(user_info_ptr, user_info_len) } {
        Some(user_info) if user_info.is_empty() || user_info == "{}" => ACCOUNT_ACTION_LOGOUT,
        Some(_) => ACCOUNT_ACTION_LOGIN,
        None => ACCOUNT_ACTION_FAILURE,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_value_action(
    value_ptr: *const u8,
    value_len: usize,
) -> i32 {
    match unsafe { read_utf8(value_ptr, value_len) } {
        Some(value) if value.is_empty() => ACCOUNT_ACTION_NONE,
        Some(_) => ACCOUNT_ACTION_APPLY,
        None => ACCOUNT_ACTION_FAILURE,
    }
}

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
pub extern "C" fn pandar_plugin_account_mutation_status(
    primary_succeeded: bool,
    secondary_succeeded: bool,
) -> i32 {
    if primary_succeeded && secondary_succeeded {
        0
    } else {
        ABI_INVALID_RESULT
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_mutation_result(
    primary_succeeded: bool,
    secondary_succeeded: bool,
    error_ptr: *const u8,
    error_len: usize,
) -> PluginHttpResult {
    if primary_succeeded && secondary_succeeded {
        return result(0, 0, "");
    }
    let body = unsafe { read_utf8(error_ptr, error_len) }
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| stable_error_body("account_state_unavailable"));
    result(ABI_INVALID_RESULT, 0, body)
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_ticket_admission(
    ticket_ptr: *const u8,
    ticket_len: usize,
) -> PluginHttpResult {
    match unsafe { read_utf8(ticket_ptr, ticket_len) } {
        Some(ticket) if !ticket.is_empty() => result(0, 0, ""),
        _ => result(
            ABI_INVALID_RESULT,
            401,
            stable_error_body("invalid_plugin_ticket"),
        ),
    }
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
