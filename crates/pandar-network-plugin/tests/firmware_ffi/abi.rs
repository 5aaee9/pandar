use std::{ffi::c_void, slice};

use pandar_network_plugin::{PluginHttpResult, firmware::PluginFirmwareCallbackResult};

unsafe extern "C" {
    fn pandar_plugin_firmware_session_create(
        hub_url_ptr: *const u8,
        hub_url_len: usize,
        token_ptr: *const u8,
        token_len: usize,
        generation: u64,
    ) -> *mut c_void;
    fn pandar_plugin_firmware_session_update(
        session: *mut c_void,
        hub_url_ptr: *const u8,
        hub_url_len: usize,
        token_ptr: *const u8,
        token_len: usize,
        generation: u64,
    ) -> i32;
    fn pandar_plugin_firmware_observe_printers(
        session: *mut c_void,
        body_ptr: *const u8,
        body_len: usize,
        generation: u64,
        observation_sequence: u64,
    ) -> i32;
    fn pandar_plugin_firmware_catalog(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
        printer_id_ptr: *const u8,
        printer_id_len: usize,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_refresh_version(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
        printer_id_ptr: *const u8,
        printer_id_len: usize,
        sequence_id_ptr: *const u8,
        sequence_id_len: usize,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_send(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
        printer_id_ptr: *const u8,
        printer_id_len: usize,
        message_ptr: *const u8,
        message_len: usize,
        tunnel: i32,
        token_out: *mut u64,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_return_handoff(
        session: *mut c_void,
        token: u64,
        origin_tick: u64,
    ) -> i32;
    fn pandar_plugin_firmware_next_status_override(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_next_callback(
        session: *mut c_void,
        timeout_ms: u64,
    ) -> PluginFirmwareCallbackResult;
    fn pandar_plugin_firmware_cancel_generation(session: *mut c_void, generation: u64);
    fn pandar_plugin_firmware_stop(session: *mut c_void);
    fn pandar_plugin_firmware_session_destroy(session: *mut c_void);
    fn pandar_plugin_free_with_capacity(ptr: *mut c_void, len: usize, cap: usize);
}

pub(super) struct Session {
    raw: *mut c_void,
}

pub(super) struct HttpOutput {
    pub(super) status: i32,
    pub(super) http_code: u32,
    pub(super) body: String,
}

pub(super) struct CallbackOutput {
    pub(super) status: i32,
    pub(super) dev_id: String,
    pub(super) message: String,
    pub(super) tunnel: i32,
}

impl Session {
    pub(super) fn create(hub_url: &str, token: &str, generation: u64) -> Self {
        let raw = unsafe {
            pandar_plugin_firmware_session_create(
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
                generation,
            )
        };
        assert!(!raw.is_null());
        Self { raw }
    }

    pub(super) fn update(&self, hub_url: &str, token: &str, generation: u64) -> i32 {
        unsafe {
            pandar_plugin_firmware_session_update(
                self.raw,
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
                generation,
            )
        }
    }

    pub(super) fn observe(&self, body: &str, generation: u64, observation_sequence: u64) -> i32 {
        unsafe {
            pandar_plugin_firmware_observe_printers(
                self.raw,
                body.as_ptr(),
                body.len(),
                generation,
                observation_sequence,
            )
        }
    }

    pub(super) fn catalog(&self, dev_id: &str, printer_id: &str) -> HttpOutput {
        let result = unsafe {
            pandar_plugin_firmware_catalog(
                self.raw,
                dev_id.as_ptr(),
                dev_id.len(),
                printer_id.as_ptr(),
                printer_id.len(),
            )
        };
        take_http(result)
    }

    pub(super) fn refresh(&self, dev_id: &str, printer_id: &str, sequence_id: &str) -> HttpOutput {
        let result = unsafe {
            pandar_plugin_firmware_refresh_version(
                self.raw,
                dev_id.as_ptr(),
                dev_id.len(),
                printer_id.as_ptr(),
                printer_id.len(),
                sequence_id.as_ptr(),
                sequence_id.len(),
            )
        };
        take_http(result)
    }

    pub(super) fn send(
        &self,
        dev_id: &str,
        printer_id: &str,
        message: &str,
        tunnel: i32,
        token_out: Option<&mut u64>,
    ) -> HttpOutput {
        let token_out = token_out.map_or(std::ptr::null_mut(), |token| token as *mut u64);
        let result = unsafe {
            pandar_plugin_firmware_send(
                self.raw,
                dev_id.as_ptr(),
                dev_id.len(),
                printer_id.as_ptr(),
                printer_id.len(),
                message.as_ptr(),
                message.len(),
                tunnel,
                token_out,
            )
        };
        take_http(result)
    }

    pub(super) fn handoff(&self, token: u64, origin_tick: u64) -> i32 {
        unsafe { pandar_plugin_firmware_return_handoff(self.raw, token, origin_tick) }
    }

    pub(super) fn next_status(&self, dev_id: &str) -> HttpOutput {
        let result = unsafe {
            pandar_plugin_firmware_next_status_override(self.raw, dev_id.as_ptr(), dev_id.len())
        };
        take_http(result)
    }

    pub(super) fn next_callback(&self, timeout_ms: u64) -> CallbackOutput {
        next_callback(self.raw as usize, timeout_ms)
    }

    pub(super) fn address(&self) -> usize {
        self.raw as usize
    }

    pub(super) fn cancel_generation(&self, generation: u64) {
        unsafe { pandar_plugin_firmware_cancel_generation(self.raw, generation) }
    }

    pub(super) fn stop(&self) {
        unsafe { pandar_plugin_firmware_stop(self.raw) }
    }

    pub(super) fn destroy(self) {
        unsafe { pandar_plugin_firmware_session_destroy(self.raw) }
    }
}

pub(super) fn next_callback(session: usize, timeout_ms: u64) -> CallbackOutput {
    let result =
        unsafe { pandar_plugin_firmware_next_callback(session as *mut c_void, timeout_ms) };
    take_callback(result)
}

fn take_http(result: PluginHttpResult) -> HttpOutput {
    let body = copy_allocation(result.body_ptr, result.body_len, result.body_cap);
    HttpOutput {
        status: result.status,
        http_code: result.http_code,
        body,
    }
}

fn take_callback(result: PluginFirmwareCallbackResult) -> CallbackOutput {
    let dev_id = copy_allocation(result.dev_id_ptr, result.dev_id_len, result.dev_id_cap);
    let message = copy_allocation(result.message_ptr, result.message_len, result.message_cap);
    CallbackOutput {
        status: result.status,
        dev_id,
        message,
        tunnel: result.tunnel,
    }
}

fn copy_allocation(ptr: *mut u8, len: usize, cap: usize) -> String {
    assert!(len <= cap);
    if len == 0 {
        assert_eq!(cap, 0);
        return String::new();
    }
    assert!(!ptr.is_null());
    let value = unsafe { String::from_utf8(slice::from_raw_parts(ptr, len).to_vec()).unwrap() };
    unsafe { pandar_plugin_free_with_capacity(ptr.cast(), len, cap) };
    value
}
