use super::{PluginHttpResult, RequestKind, invalid_input, normalize_hub_url, read_utf8};
use crate::http::{plugin_auto_nozzle_mapping_url, post_json};

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_submit_h2c_auto_nozzle_mapping(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    request_json_ptr: *const u8,
    request_json_len: usize,
) -> PluginHttpResult {
    let (Some(hub_url), Some(token), Some(printer_id), Some(request_json)) = (
        unsafe { read_utf8(hub_url_ptr, hub_url_len) },
        unsafe { read_utf8(token_ptr, token_len) },
        unsafe { read_utf8(printer_id_ptr, printer_id_len) },
        unsafe { read_utf8(request_json_ptr, request_json_len) },
    ) else {
        return invalid_input("bad_request");
    };
    submit_h2c_auto_nozzle_mapping_upstream(&hub_url, &token, &printer_id, &request_json)
}

pub(crate) fn submit_h2c_auto_nozzle_mapping_upstream(
    hub_url: &str,
    token: &str,
    printer_id: &str,
    request_json: &str,
) -> PluginHttpResult {
    let Some(hub_url) = normalize_hub_url(hub_url.to_owned()) else {
        return invalid_input("invalid_hub_url");
    };
    if token.trim().is_empty() {
        return invalid_input("invalid_auth_token");
    }
    if printer_id.trim().is_empty() {
        return invalid_input("invalid_printer_id");
    }
    let Some(request) =
        serde_json::from_str::<pandar_core::H2cAutoNozzleMappingRequest>(request_json)
            .ok()
            .filter(pandar_core::H2cAutoNozzleMappingRequest::is_valid)
    else {
        return invalid_input("invalid_printer_operation");
    };
    let Some(url) = plugin_auto_nozzle_mapping_url(&hub_url, printer_id) else {
        return invalid_input("invalid_printer_id");
    };

    post_json(
        url.as_str(),
        Some(token),
        request,
        RequestKind::H2cAutoNozzleMapping,
    )
}
