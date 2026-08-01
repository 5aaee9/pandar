use super::{PluginHttpResult, RequestKind, invalid_input, normalize_hub_url, read_utf8};
use crate::http::{plugin_auto_nozzle_mapping_url, post_json};

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_submit_h2c_auto_nozzle_mapping(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    request_json_ptr: *const u8,
    request_json_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len).filter(|token| !token.trim().is_empty())
    else {
        return invalid_input("invalid_auth_token");
    };
    let Some(printer_id) = read_utf8(printer_id_ptr, printer_id_len)
        .filter(|printer_id| !printer_id.trim().is_empty())
    else {
        return invalid_input("invalid_printer_id");
    };
    let Some(request) = read_utf8(request_json_ptr, request_json_len)
        .and_then(|body| {
            serde_json::from_str::<pandar_core::H2cAutoNozzleMappingRequest>(&body).ok()
        })
        .filter(pandar_core::H2cAutoNozzleMappingRequest::is_valid)
    else {
        return invalid_input("invalid_printer_operation");
    };
    let Some(url) = plugin_auto_nozzle_mapping_url(&hub_url, &printer_id) else {
        return invalid_input("invalid_printer_id");
    };

    post_json(
        url.as_str(),
        Some(&token),
        request,
        RequestKind::H2cAutoNozzleMapping,
    )
}
