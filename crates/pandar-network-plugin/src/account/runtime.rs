use crate::normalize_hub_url;

type RuntimeConfigVisitor =
    unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, usize, bool, *const u8, usize, bool);

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_runtime_config(
    context: *mut std::ffi::c_void,
    visitor: Option<RuntimeConfigVisitor>,
) -> i32 {
    let Some(visitor) = visitor else {
        return -1;
    };
    let (hub_url, hub_configured) = env_or_default(
        "PANDAR_PLUGIN_HUB_URL",
        "APP_API_URL",
        "http://127.0.0.1:8080",
    );
    let hub_url = canonical_hub_identity(&hub_url);
    let (frontend_url, frontend_configured) = env_or_default(
        "PANDAR_PLUGIN_FRONTEND_URL",
        "APP_BASE_URL",
        "http://localhost:3000",
    );
    unsafe {
        visitor(
            context,
            hub_url.as_ptr(),
            hub_url.len(),
            hub_configured,
            frontend_url.as_ptr(),
            frontend_url.len(),
            frontend_configured,
        );
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_debug_consistent(studio_debug: bool) -> bool {
    !studio_debug
}

fn env_or_default(primary: &str, secondary: &str, fallback: &str) -> (String, bool) {
    [primary, secondary]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .map_or_else(|| (fallback.to_owned(), false), |value| (value, true))
}

/// Whether any explicit plugin Web/Hub URL environment configuration is set; such
/// configuration outranks a saved manual server selection.
pub(super) fn url_environment_configured() -> bool {
    [
        "PANDAR_PLUGIN_HUB_URL",
        "APP_API_URL",
        "PANDAR_PLUGIN_FRONTEND_URL",
        "APP_BASE_URL",
    ]
    .into_iter()
    .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

pub(super) fn canonical_hub_identity(value: &str) -> String {
    normalize_hub_url(value.to_owned()).unwrap_or_else(|| value.trim().trim_end_matches('/').into())
}
