use super::{ABI_CONNECT_FAILED, ABI_GET_USER_PRINT_INFO_FAILED, ABI_INVALID_RESULT};
use crate::{PluginHttpResult, read_utf8, result, stable_error_body};

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_print_info_result(
    upstream_status: i32,
    upstream_http_code: u32,
    upstream_body_ptr: *const u8,
    upstream_body_len: usize,
    snapshot_current: bool,
) -> PluginHttpResult {
    if upstream_status != 0 {
        return unsafe {
            copied_result(
                ABI_GET_USER_PRINT_INFO_FAILED,
                upstream_http_code,
                upstream_body_ptr,
                upstream_body_len,
            )
        };
    }
    if !snapshot_current {
        return result(
            ABI_GET_USER_PRINT_INFO_FAILED,
            401,
            stable_error_body("invalid_auth_token"),
        );
    }
    unsafe { copied_result(0, upstream_http_code, upstream_body_ptr, upstream_body_len) }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_firmware_catalog_result(
    upstream_status: i32,
    upstream_http_code: u32,
    upstream_body_ptr: *const u8,
    upstream_body_len: usize,
    snapshot_current: bool,
) -> PluginHttpResult {
    if !snapshot_current {
        return result(
            ABI_INVALID_RESULT,
            409,
            stable_error_body("stale_firmware_catalog"),
        );
    }
    unsafe {
        copied_result(
            if upstream_status == 0 {
                0
            } else {
                ABI_INVALID_RESULT
            },
            upstream_http_code,
            upstream_body_ptr,
            upstream_body_len,
        )
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_printer_operation_result(
    upstream_status: i32,
    upstream_http_code: u32,
    upstream_body_ptr: *const u8,
    upstream_body_len: usize,
    snapshot_current: bool,
) -> PluginHttpResult {
    // SAFETY: the shim passes a borrowed body valid for this call.
    let body = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            upstream_body_ptr,
            upstream_body_len,
        ))
        .unwrap_or_default()
        .to_owned()
    };
    studio_printer_operation_result(upstream_status, upstream_http_code, body, snapshot_current)
}

pub(crate) fn studio_printer_operation_result(
    upstream_status: i32,
    upstream_http_code: u32,
    upstream_body: String,
    snapshot_current: bool,
) -> PluginHttpResult {
    if !snapshot_current {
        return result(
            ABI_INVALID_RESULT,
            409,
            stable_error_body("stale_printer_operation"),
        );
    }
    unsafe {
        copied_result(
            if upstream_status == 0 {
                0
            } else {
                ABI_INVALID_RESULT
            },
            upstream_http_code,
            upstream_body.as_ptr(),
            upstream_body.len(),
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_status_delivery_result(delivered: bool) -> PluginHttpResult {
    if delivered {
        result(0, 0, "")
    } else {
        result(
            ABI_CONNECT_FAILED,
            0,
            stable_error_body("studio_status_undelivered"),
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_file_transfer_unavailable() -> PluginHttpResult {
    result(
        ABI_INVALID_RESULT,
        501,
        stable_error_body("unsupported_file_transfer"),
    )
}

unsafe fn copied_result(
    status: i32,
    http_code: u32,
    body_ptr: *const u8,
    body_len: usize,
) -> PluginHttpResult {
    let Some(body) = (unsafe { read_utf8(body_ptr, body_len) }) else {
        return result(
            ABI_INVALID_RESULT,
            0,
            stable_error_body("invalid_plugin_response"),
        );
    };
    result(status, http_code, body)
}
