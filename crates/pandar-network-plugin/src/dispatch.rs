//! Coarse-grained Studio dispatch policy.
//!
//! The C++ shim keeps only the callback gates and STL adapters: it exposes the
//! Studio `std::function` callbacks, the callback gate, Rust-owned firmware
//! generation checks, and the clock sources through one flat
//! [`PluginDispatchBridge`] vtable. This module owns the routing and
//! scheduling the shim used to sequence inline: which classified message
//! reaches which session, HTTP, or firmware path; how a prepared delivery is
//! claimed and invoked; what one heartbeat iteration does; and the firmware
//! callback handoff window.

use std::ffi::c_void;
use std::time::Duration;

use crate::connection::ffi::session as connection_session;
use crate::connection::{
    ConnectionSession, ShimCallbackBridge, StudioRequestSnapshot, normalize_studio_dev_id,
    pandar_plugin_shim_dispatch_connection_transition,
    pandar_plugin_shim_dispatch_offline_deliveries,
};
use crate::firmware::{FirmwareSendOutcome, FirmwareTunnel, session_ref as firmware_session_ref};
use crate::studio_message::{StudioMessageRoute, classify_studio_message};
use crate::studio_policy::{studio_printer_operation_result, studio_request_admitted};
use crate::{
    PluginHttpResult, h2c::submit_h2c_auto_nozzle_mapping_upstream, read_utf8,
    submit_printer_operation_upstream,
};

const ABI_SUCCESS: i32 = 0;
const ABI_CONNECT_FAILED: i32 = -2;
const ABI_INVALID_RESULT: i32 = -19;

const STUDIO_WORK_CLOUD_MESSAGE: i32 = 1;
const STUDIO_WORK_LOCAL_MESSAGE: i32 = 2;

const CLOUD_TUNNEL: i32 = 0;
const LOCAL_TUNNEL: i32 = 1;

const FIRMWARE_CALLBACK_WAIT_MS: u64 = 25;
const FIRMWARE_CALLBACK_DEADLINE_NS: u64 = 2_000_000_000;
const NO_AUTH_RETRY_DELAY_MS: u32 = 2_000;

#[repr(C)]
pub struct PluginDispatchBridge {
    pub base: ShimCallbackBridge,
    pub firmware_generation_current: extern "C" fn(*mut c_void, u64) -> i32,
    pub gate_try_lock_until: extern "C" fn(*mut c_void, u64) -> i32,
    pub steady_tick_ns: extern "C" fn(*mut c_void) -> u64,
    pub now_ms: extern "C" fn(*mut c_void) -> u64,
    pub refresh_local_webserver: extern "C" fn(*mut c_void),
    pub trace: extern "C" fn(*mut c_void, *const u8, usize),
    pub invoke_http_error: extern "C" fn(*mut c_void, u32, *const u8, usize),
    pub sync_firmware: extern "C" fn(*mut c_void, *mut c_void) -> i32,
    pub retry_no_auth: extern "C" fn(*mut c_void),
    pub invoke_local_connected_with_body:
        extern "C" fn(*mut c_void, i32, *const u8, usize, *const u8, usize) -> i32,
}

#[repr(C)]
pub struct PluginDispatchMessageRequest {
    pub session: *mut c_void,
    pub firmware_session: *mut c_void,
    pub firmware_generation: u64,
    pub tunnel: i32,
    pub local_generation: u64,
    pub dev_id_ptr: *const u8,
    pub dev_id_len: usize,
    pub message_ptr: *const u8,
    pub message_len: usize,
}

#[repr(C)]
pub struct PluginPendingOutcome {
    /// Studio heartbeat plan wait for the next iteration; `u32::MAX` means
    /// wait until the dispatcher is woken.
    pub wait_ms: u32,
    /// `1` when the agent has no account token, so the caller polls the
    /// no-auth retry window instead of sleeping indefinitely.
    pub logged_out: i32,
}

unsafe fn bridge<'a>(ptr: *const PluginDispatchBridge) -> Option<&'a PluginDispatchBridge> {
    // SAFETY: the shim passes a pointer to its static bridge instance, which
    // outlives every dispatch call.
    unsafe { ptr.as_ref() }
}

struct CallbackGate<'a> {
    bridge: &'a PluginDispatchBridge,
    agent: *mut c_void,
}

impl<'a> CallbackGate<'a> {
    fn lock(bridge: &'a PluginDispatchBridge, agent: *mut c_void) -> Self {
        (bridge.base.gate_lock)(agent);
        Self { bridge, agent }
    }
}

impl Drop for CallbackGate<'_> {
    fn drop(&mut self) {
        (self.bridge.base.gate_unlock)(self.agent);
    }
}

fn trace(bridge: &PluginDispatchBridge, agent: *mut c_void, message: &str) {
    (bridge.trace)(agent, message.as_ptr(), message.len());
}

fn tunnel_work_kind(tunnel: i32) -> i32 {
    if tunnel == CLOUD_TUNNEL {
        STUDIO_WORK_CLOUD_MESSAGE
    } else {
        STUDIO_WORK_LOCAL_MESSAGE
    }
}

fn tunnel_label(tunnel: i32) -> &'static str {
    if tunnel == CLOUD_TUNNEL {
        "cloud"
    } else {
        "local"
    }
}

/// Consumes an in-crate `PluginHttpResult`, returning `(status, http_code, body)`.
fn take_http(result: PluginHttpResult) -> (i32, u32, String) {
    let mut result = result;
    let body = if result.body_ptr.is_null() {
        String::new()
    } else {
        // SAFETY: the result was produced in this crate by `result()`, which
        // forgets a `Vec<u8>` of exactly `body_len`/`body_cap`.
        let bytes =
            unsafe { Vec::from_raw_parts(result.body_ptr, result.body_len, result.body_cap) };
        result.body_ptr = std::ptr::null_mut();
        String::from_utf8(bytes).unwrap_or_default()
    };
    (result.status, result.http_code, body)
}

mod message;
mod pending;

pub(crate) use pending::dispatch_transition_and_tickets;
