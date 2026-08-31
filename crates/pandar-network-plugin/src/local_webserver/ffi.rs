use crate::{PluginHttpResult, invalid_input, read_utf8};

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_start_local_webserver(
    web_url_ptr: *const u8,
    web_url_len: usize,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    web_configured: bool,
    hub_configured: bool,
) -> PluginHttpResult {
    let Some(web_url) = (unsafe { read_utf8(web_url_ptr, web_url_len) }) else {
        return invalid_input("invalid_target_server");
    };
    let Some(hub_url) = (unsafe { read_utf8(hub_url_ptr, hub_url_len) }) else {
        return invalid_input("invalid_target_server");
    };
    super::start(web_url, hub_url, web_configured, hub_configured)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_base_url() -> PluginHttpResult {
    super::base_url()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_config() -> PluginHttpResult {
    super::config()
}
