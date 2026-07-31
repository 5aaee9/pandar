use std::ffi::{CStr, c_char};

use crate::{PluginHttpResult, read_utf8, result, stable_error_body, studio_status};

pub const NETWORK_AGENT_VERSION: &CStr = c"02.07.01.51";

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_network_agent_version() -> *const c_char {
    NETWORK_AGENT_VERSION.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_camera_access_result(agent_valid: bool) -> PluginHttpResult {
    if agent_valid {
        result(-19, 0, stable_error_body("camera_unavailable"))
    } else {
        result(-1, 0, stable_error_body("invalid_handle"))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_connect_json(
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    model_ptr: *const u8,
    model_len: usize,
) -> PluginHttpResult {
    let dev_id = read_utf8(dev_id_ptr, dev_id_len).unwrap_or_default();
    let model = read_utf8(model_ptr, model_len).unwrap_or_default();
    result(0, 200, studio_status::local_connect_json(&dev_id, &model))
}
