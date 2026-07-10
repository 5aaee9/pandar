use std::{ffi::c_void, sync::Mutex, time::Duration};

use anyhow::{Context, anyhow};

use crate::{
    PluginHttpResult, RequestKind, http, invalid_input, normalize_hub_url, read_utf8, result,
    runtime, stable_error_body, studio_status::validate_printer_list,
};

const STATUS_REFRESH_TIMEOUT: Duration = Duration::from_millis(750);

pub(super) struct PrinterRefreshSession {
    state: Mutex<RefreshState>,
    request: Mutex<()>,
}

struct RefreshState {
    hub_url: String,
    token: String,
    generation: u64,
}

struct RefreshSnapshot {
    hub_url: String,
    token: String,
    generation: u64,
}

struct HubResponse {
    http_code: u32,
    body: String,
}

impl PrinterRefreshSession {
    pub(super) fn new(hub_url: String, token: String) -> Self {
        Self {
            state: Mutex::new(RefreshState {
                hub_url,
                token,
                generation: 0,
            }),
            request: Mutex::new(()),
        }
    }

    pub(super) fn update(&self, hub_url: String, token: String) {
        let mut state = self.state.lock().expect("printer refresh state");
        if state.hub_url == hub_url && state.token == token {
            return;
        }
        state.hub_url = hub_url;
        state.token = token;
        state.generation = state.generation.wrapping_add(1);
    }

    pub(super) fn refresh(&self) -> PluginHttpResult {
        let Ok(_request) = self.request.try_lock() else {
            return result(1, 0, stable_error_body("hub_unavailable"));
        };
        let snapshot = {
            let state = self.state.lock().expect("printer refresh state");
            RefreshSnapshot {
                hub_url: state.hub_url.clone(),
                token: state.token.clone(),
                generation: state.generation,
            }
        };
        if snapshot.token.trim().is_empty() {
            return result(1, 400, stable_error_body("invalid_auth_token"));
        }

        let response = match fetch_printers(&snapshot) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("pandar printer status refresh failed: {error:#}");
                return result(1, 0, stable_error_body("hub_unavailable"));
            }
        };
        if !(200..300).contains(&response.http_code) {
            return result(
                1,
                response.http_code,
                http::redact_hub_error(
                    RequestKind::PrinterLookup,
                    response.http_code,
                    &response.body,
                ),
            );
        }
        if let Err(error) = validate_printer_list(&response.body)
            .context("validate Hub printer status refresh response")
        {
            eprintln!("pandar printer status refresh failed: {error:#}");
            return result(1, response.http_code, stable_error_body("invalid_response"));
        }
        let state = self.state.lock().expect("printer refresh state");
        if state.generation != snapshot.generation {
            let error = anyhow!("printer refresh credentials changed during request");
            eprintln!("pandar printer status refresh discarded: {error:#}");
            return result(1, 0, stable_error_body("hub_unavailable"));
        }
        result(0, response.http_code, response.body)
    }
}

fn fetch_printers(snapshot: &RefreshSnapshot) -> anyhow::Result<HubResponse> {
    runtime().block_on(async {
        tokio::time::timeout(STATUS_REFRESH_TIMEOUT, async {
            let client = reqwest::Client::builder()
                .timeout(STATUS_REFRESH_TIMEOUT)
                .build()
                .context("build Hub printer status refresh client")?;
            let response = client
                .get(format!("{}/api/v1/plugin/printers", snapshot.hub_url))
                .bearer_auth(&snapshot.token)
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .context("send Hub printer status refresh request")?;
            let http_code = response.status().as_u16().into();
            let body = response
                .text()
                .await
                .map_err(reqwest::Error::without_url)
                .context("read Hub printer status refresh response")?;
            Ok(HubResponse { http_code, body })
        })
        .await
        .context("Hub printer status refresh timed out")?
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_refresh_session_create(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> *mut c_void {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return std::ptr::null_mut();
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(PrinterRefreshSession::new(hub_url, token))).cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_refresh_session_update(
    session: *mut c_void,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session.cast::<PrinterRefreshSession>().as_ref() }) else {
        return 1;
    };
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return 1;
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
        return 1;
    };
    session.update(hub_url, token);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_refresh(session: *mut c_void) -> PluginHttpResult {
    let Some(session) = (unsafe { session.cast::<PrinterRefreshSession>().as_ref() }) else {
        return invalid_input("invalid_printer_refresh_session");
    };
    session.refresh()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_refresh_session_destroy(session: *mut c_void) {
    if !session.is_null() {
        unsafe {
            drop(Box::from_raw(session.cast::<PrinterRefreshSession>()));
        }
    }
}
