use std::{ffi::c_void, time::Duration};

use serde::Serialize;

mod callback_result;

use super::{
    callbacks::FirmwareTunnel,
    model::{FirmwareSendOutcome, StudioFirmwareParse},
    parser::parse_studio_firmware,
    session::FirmwarePluginSession,
};
use crate::{
    PluginHttpResult, invalid_input, normalize_hub_url, read_utf8, result, stable_error_body,
};
use callback_result::{callback_result, empty_callback};

const CALLBACK_NONE: i32 = 1;

#[repr(C)]
pub struct PluginFirmwareCallbackResult {
    pub status: i32,
    pub generation: u64,
    pub origin_tick: u64,
    pub local_generation: u64,
    pub cache_generation: u64,
    pub dev_id_ptr: *mut u8,
    pub dev_id_len: usize,
    pub dev_id_cap: usize,
    pub message_ptr: *mut u8,
    pub message_len: usize,
    pub message_cap: usize,
    pub tunnel: i32,
}

#[derive(Serialize)]
struct SendOutcomeBody {
    outcome: &'static str,
}

#[unsafe(no_mangle)]
/// # Safety
/// String pointers must be valid for their corresponding lengths.
pub unsafe extern "C" fn pandar_plugin_firmware_session_create(
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
    Box::into_raw(Box::new(FirmwarePluginSession::new(hub_url, token, 1))).cast()
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live and string pointers valid for their lengths.
pub unsafe extern "C" fn pandar_plugin_firmware_session_sync_account(
    session: *mut c_void,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> u64 {
    let Some(session) = (unsafe { session_ref(session) }) else {
        return 0;
    };
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return 0;
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
        return 0;
    };
    session.sync_account(hub_url, token)
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live and string pointers valid for their lengths.
pub unsafe extern "C" fn pandar_plugin_firmware_session_fence_account(
    session: *mut c_void,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> u64 {
    let Some(session) = (unsafe { session_ref(session) }) else {
        return 0;
    };
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return 0;
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
        return 0;
    };
    session.fence_account(hub_url, token)
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must point to a live firmware session.
pub unsafe extern "C" fn pandar_plugin_firmware_session_generation(session: *mut c_void) -> u64 {
    unsafe { session_ref(session) }.map_or(0, FirmwarePluginSession::generation)
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must point to a live firmware session.
pub unsafe extern "C" fn pandar_plugin_firmware_session_generation_current(
    session: *mut c_void,
    expected: u64,
) -> i32 {
    unsafe { session_ref(session) }.is_some_and(|session| session.generation_is_current(expected))
        as i32
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live and string pointers valid for their lengths.
pub unsafe extern "C" fn pandar_plugin_firmware_catalog(
    session: *mut c_void,
    studio_dev_id_ptr: *const u8,
    studio_dev_id_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    expected_generation: u64,
) -> PluginHttpResult {
    let Some((session, studio_dev_id, printer_id)) = (unsafe {
        request_parts(
            session,
            studio_dev_id_ptr,
            studio_dev_id_len,
            printer_id_ptr,
            printer_id_len,
        )
    }) else {
        return invalid_input("invalid_firmware_request");
    };
    match session.catalog_json(&studio_dev_id, &printer_id, expected_generation) {
        Ok(body) => result(0, 200, body),
        Err(error) => {
            eprintln!("pandar firmware catalog request failed: {error:#}");
            result(1, 0, stable_error_body("hub_unavailable"))
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live and string pointers valid for their lengths.
pub unsafe extern "C" fn pandar_plugin_firmware_refresh_version(
    session: *mut c_void,
    studio_dev_id_ptr: *const u8,
    studio_dev_id_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    sequence_id_ptr: *const u8,
    sequence_id_len: usize,
    expected_generation: u64,
) -> PluginHttpResult {
    let Some((session, _studio_dev_id, printer_id)) = (unsafe {
        request_parts(
            session,
            studio_dev_id_ptr,
            studio_dev_id_len,
            printer_id_ptr,
            printer_id_len,
        )
    }) else {
        return invalid_input("invalid_firmware_request");
    };
    let Some(sequence_id) = read_utf8(sequence_id_ptr, sequence_id_len) else {
        return invalid_input("invalid_firmware_request");
    };
    result(
        0,
        200,
        session.refresh_version_json(&printer_id, &sequence_id, expected_generation),
    )
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live, string pointers valid, and `token_out` writable.
pub unsafe extern "C" fn pandar_plugin_firmware_send(
    session: *mut c_void,
    studio_dev_id_ptr: *const u8,
    studio_dev_id_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    message_ptr: *const u8,
    message_len: usize,
    tunnel: i32,
    token_out: *mut u64,
    expected_generation: u64,
) -> PluginHttpResult {
    if token_out.is_null() {
        return invalid_input("invalid_firmware_request");
    }
    unsafe { *token_out = 0 };
    let Some((session, studio_dev_id, printer_id)) = (unsafe {
        request_parts(
            session,
            studio_dev_id_ptr,
            studio_dev_id_len,
            printer_id_ptr,
            printer_id_len,
        )
    }) else {
        return invalid_input("invalid_firmware_request");
    };
    let Some(message) = read_utf8(message_ptr, message_len) else {
        return invalid_input("invalid_firmware_request");
    };
    let Some(tunnel) = tunnel_from_ffi(tunnel) else {
        return invalid_input("invalid_firmware_request");
    };
    match parse_studio_firmware(&message) {
        StudioFirmwareParse::NotFirmware => result(2, 200, String::new()),
        StudioFirmwareParse::InvalidFirmware => invalid_input("unsupported_printer_operation"),
        StudioFirmwareParse::Firmware(_) => {
            let response = session.send(
                &studio_dev_id,
                &printer_id,
                &message,
                tunnel,
                expected_generation,
            );
            if let Some(token) = response.callback_token {
                unsafe { *token_out = token };
            }
            send_result(response.outcome)
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must point to a live firmware session.
pub unsafe extern "C" fn pandar_plugin_firmware_return_handoff(
    session: *mut c_void,
    token: u64,
    origin_tick: u64,
    local_generation: u64,
    cache_generation: u64,
) -> i32 {
    let Some(session) = (unsafe { session_ref(session) }) else {
        return 1;
    };
    i32::from(!session.return_handoff_at(
        token,
        origin_tick,
        local_generation,
        cache_generation,
        std::time::Instant::now(),
    ))
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be live and `studio_dev_id_ptr` valid for its length.
pub unsafe extern "C" fn pandar_plugin_firmware_next_status_override(
    session: *mut c_void,
    studio_dev_id_ptr: *const u8,
    studio_dev_id_len: usize,
) -> PluginHttpResult {
    let Some(session) = (unsafe { session_ref(session) }) else {
        return invalid_input("invalid_firmware_session");
    };
    let Some(dev_id) = read_utf8(studio_dev_id_ptr, studio_dev_id_len) else {
        return invalid_input("invalid_firmware_request");
    };
    session.next_status_override(&dev_id).map_or_else(
        || result(1, 204, String::new()),
        |body| result(0, 200, body),
    )
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must point to a live firmware session.
/// On status `0`, the caller owns both `dev_id` and `message` allocations and must free each once
/// with `pandar_plugin_free_with_capacity(ptr, len, cap)`. Other statuses return no allocations.
pub unsafe extern "C" fn pandar_plugin_firmware_next_callback(
    session: *mut c_void,
    timeout_ms: u64,
) -> PluginFirmwareCallbackResult {
    let Some(session) = (unsafe { session_ref(session) }) else {
        return empty_callback();
    };
    session
        .wait_ready_callback(Duration::from_millis(timeout_ms))
        .map_or_else(empty_callback, callback_result)
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be null or point to a live firmware session.
pub unsafe extern "C" fn pandar_plugin_firmware_cancel_generation(
    session: *mut c_void,
    generation: u64,
) {
    if let Some(session) = unsafe { session_ref(session) } {
        session.cancel_generation(generation);
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be null or point to a live firmware session.
pub unsafe extern "C" fn pandar_plugin_firmware_stop(session: *mut c_void) {
    if let Some(session) = unsafe { session_ref(session) } {
        session.stop();
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be null or returned by session creation exactly once. All active calls must have
/// returned after stop/join, and the pointer must not be used again.
pub unsafe extern "C" fn pandar_plugin_firmware_session_destroy(session: *mut c_void) {
    if !session.is_null() {
        let session = unsafe { Box::from_raw(session.cast::<FirmwarePluginSession>()) };
        session.stop();
        drop(session);
    }
}

unsafe fn request_parts<'a>(
    session: *mut c_void,
    studio_dev_id_ptr: *const u8,
    studio_dev_id_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
) -> Option<(&'a FirmwarePluginSession, String, String)> {
    let session = unsafe { session_ref(session) }?;
    let studio_dev_id = read_utf8(studio_dev_id_ptr, studio_dev_id_len)?;
    let printer_id = read_utf8(printer_id_ptr, printer_id_len)?;
    if studio_dev_id.trim().is_empty() || printer_id.trim().is_empty() {
        return None;
    }
    Some((session, studio_dev_id, printer_id))
}

pub(crate) unsafe fn session_ref<'a>(session: *mut c_void) -> Option<&'a FirmwarePluginSession> {
    unsafe { session.cast::<FirmwarePluginSession>().as_ref() }
}

fn tunnel_from_ffi(tunnel: i32) -> Option<FirmwareTunnel> {
    match tunnel {
        0 => Some(FirmwareTunnel::Cloud),
        1 => Some(FirmwareTunnel::Local),
        _ => None,
    }
}

fn send_result(outcome: FirmwareSendOutcome) -> PluginHttpResult {
    let (status, body) = match outcome {
        FirmwareSendOutcome::Acknowledged => (0, "acknowledged"),
        FirmwareSendOutcome::Rejected => (0, "rejected"),
        FirmwareSendOutcome::PublishedWithoutAcknowledgement => {
            (0, "published_without_acknowledgement")
        }
        FirmwareSendOutcome::OutcomeUnknown => (0, "firmware_outcome_unknown"),
        FirmwareSendOutcome::PrePublishFailure => (1, "pre_publish_failure"),
    };
    result(
        status,
        if status == 0 { 200 } else { 400 },
        serde_json::to_string(&SendOutcomeBody { outcome: body })
            .expect("firmware send outcome is serializable"),
    )
}
