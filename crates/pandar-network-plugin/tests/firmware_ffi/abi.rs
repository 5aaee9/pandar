use std::{ffi::c_void, slice};

use pandar_network_plugin::{
    PluginHttpResult,
    firmware::{FirmwarePluginSession, PluginFirmwareCallbackResult},
    studio_status::project_hub_printers,
};

unsafe extern "C" {
    fn pandar_plugin_firmware_session_create(
        hub_url_ptr: *const u8,
        hub_url_len: usize,
        token_ptr: *const u8,
        token_len: usize,
    ) -> *mut c_void;
    fn pandar_plugin_firmware_session_sync_account(
        session: *mut c_void,
        hub_url_ptr: *const u8,
        hub_url_len: usize,
        token_ptr: *const u8,
        token_len: usize,
    ) -> u64;
    fn pandar_plugin_firmware_session_fence_account(
        session: *mut c_void,
        hub_url_ptr: *const u8,
        hub_url_len: usize,
        token_ptr: *const u8,
        token_len: usize,
    ) -> u64;
    fn pandar_plugin_firmware_catalog(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
        printer_id_ptr: *const u8,
        printer_id_len: usize,
        expected_generation: u64,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_refresh_version(
        session: *mut c_void,
        dev_id_ptr: *const u8,
        dev_id_len: usize,
        printer_id_ptr: *const u8,
        printer_id_len: usize,
        sequence_id_ptr: *const u8,
        sequence_id_len: usize,
        expected_generation: u64,
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
        expected_generation: u64,
    ) -> PluginHttpResult;
    fn pandar_plugin_firmware_return_handoff(
        session: *mut c_void,
        token: u64,
        origin_tick: u64,
        local_generation: u64,
        cache_generation: u64,
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
    pub(super) generation: u64,
    pub(super) local_generation: u64,
    pub(super) cache_generation: u64,
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
            )
        };
        assert!(!raw.is_null());
        let session = Self { raw };
        for next_generation in 2..=generation {
            assert_eq!(session.fence(hub_url, token, next_generation), 0);
        }
        session
    }

    pub(super) fn update(&self, hub_url: &str, token: &str, generation: u64) -> i32 {
        let updated = unsafe {
            pandar_plugin_firmware_session_sync_account(
                self.raw,
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
            )
        };
        i32::from(updated != generation)
    }

    pub(super) fn fence(&self, hub_url: &str, token: &str, generation: u64) -> i32 {
        let updated = unsafe {
            pandar_plugin_firmware_session_fence_account(
                self.raw,
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
            )
        };
        i32::from(updated != generation)
    }

    pub(super) fn observe(&self, body: &str, generation: u64, observation_sequence: u64) -> i32 {
        let Ok(projection) = project_hub_printers(body) else {
            return 1;
        };
        let session = unsafe { &*self.raw.cast::<FirmwarePluginSession>() };
        i32::from(
            session
                .observe_printers(
                    &projection.into_firmware(),
                    generation,
                    observation_sequence,
                )
                .is_err(),
        )
    }

    pub(super) fn catalog(
        &self,
        dev_id: &str,
        printer_id: &str,
        expected_generation: u64,
    ) -> HttpOutput {
        let result = unsafe {
            pandar_plugin_firmware_catalog(
                self.raw,
                dev_id.as_ptr(),
                dev_id.len(),
                printer_id.as_ptr(),
                printer_id.len(),
                expected_generation,
            )
        };
        take_http(result)
    }

    pub(super) fn refresh(
        &self,
        dev_id: &str,
        printer_id: &str,
        sequence_id: &str,
        expected_generation: u64,
    ) -> HttpOutput {
        let result = unsafe {
            pandar_plugin_firmware_refresh_version(
                self.raw,
                dev_id.as_ptr(),
                dev_id.len(),
                printer_id.as_ptr(),
                printer_id.len(),
                sequence_id.as_ptr(),
                sequence_id.len(),
                expected_generation,
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
        expected_generation: u64,
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
                expected_generation,
            )
        };
        take_http(result)
    }

    pub(super) fn handoff(
        &self,
        token: u64,
        origin_tick: u64,
        local_generation: u64,
        cache_generation: u64,
    ) -> i32 {
        unsafe {
            pandar_plugin_firmware_return_handoff(
                self.raw,
                token,
                origin_tick,
                local_generation,
                cache_generation,
            )
        }
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
        generation: result.generation,
        local_generation: result.local_generation,
        cache_generation: result.cache_generation,
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
