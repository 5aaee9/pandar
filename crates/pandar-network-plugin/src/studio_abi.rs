use std::ffi::c_char;

use crate::{PluginHttpResult, read_utf8, result, stable_error_body, studio_status};

pub const STUDIO_ABI_SERIES: &str = env!("PANDAR_STUDIO_ABI_SERIES_ID");
pub const NETWORK_AGENT_VERSION: &str = concat!(env!("PANDAR_NETWORK_AGENT_VERSION"), "\0");

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_network_agent_version() -> *const c_char {
    NETWORK_AGENT_VERSION.as_ptr().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_sync_ams_filaments(agent_valid: bool) -> PluginHttpResult {
    if agent_valid {
        result(-32, 0, stable_error_body("unsupported_ams_sync"))
    } else {
        result(-1, 0, stable_error_body("invalid_handle"))
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_local_connect_json(
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    model_ptr: *const u8,
    model_len: usize,
) -> PluginHttpResult {
    let dev_id = unsafe { read_utf8(dev_id_ptr, dev_id_len) }.unwrap_or_default();
    let model = unsafe { read_utf8(model_ptr, model_len) }.unwrap_or_default();
    result(0, 200, studio_status::local_connect_json(&dev_id, &model))
}
