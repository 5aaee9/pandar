#![allow(non_snake_case)]

mod abi;
mod config;
mod error;
mod reader;
mod tunnel;

use std::ffi::{CStr, c_char, c_int, c_ulong, c_void};

use abi::{
    BAMBU_INVALID, BAMBU_STREAM_END, BAMBU_SUCCESS, BambuDuration, BambuSample, BambuSessionStat,
    BambuStreamInfo, Logger, PlatformChar, StreamInfoCallback, TrackReporter,
};
use config::parse_relay_url;
use error::last_error_message;
use tunnel::Tunnel;

#[unsafe(no_mangle)]
pub extern "C" fn pandar_bambu_source_sentinel() -> u32 {
    1
}

#[unsafe(no_mangle)]
/// # Safety
/// `tunnel` must be writable and `path` must point to a NUL-terminated string.
pub unsafe extern "C" fn Bambu_Create(tunnel: *mut *mut c_void, path: *const c_char) -> c_int {
    if tunnel.is_null() || path.is_null() {
        return BAMBU_INVALID;
    }
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return BAMBU_INVALID;
    };
    let Some(config) = parse_relay_url(path) else {
        return BAMBU_INVALID;
    };
    unsafe { tunnel.write(Box::into_raw(Box::new(Tunnel::new(config))).cast()) };
    BAMBU_SUCCESS
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`. The callback and context
/// must remain valid until replaced or the tunnel is destroyed.
pub unsafe extern "C" fn Bambu_SetLogger(
    tunnel: *mut c_void,
    logger: Option<Logger>,
    context: *mut c_void,
) {
    if let Some(tunnel) = unsafe { tunnel_ref(tunnel) } {
        tunnel.set_logger(logger, context);
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`. The callback and context
/// must remain valid until replaced or the tunnel is destroyed.
pub unsafe extern "C" fn Bambu_SetStreamInfoCallback(
    tunnel: *mut c_void,
    callback: Option<StreamInfoCallback>,
    context: *mut c_void,
) {
    if let Some(tunnel) = unsafe { tunnel_ref(tunnel) } {
        tunnel.set_stream_info_callback(callback, context);
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`. The callback and context
/// must remain valid until replaced or the tunnel is destroyed.
pub unsafe extern "C" fn Bambu_SetTrackReporter(
    tunnel: *mut c_void,
    reporter: Option<TrackReporter>,
    context: *mut c_void,
) {
    if let Some(tunnel) = unsafe { tunnel_ref(tunnel) } {
        tunnel.set_track_reporter(reporter, context);
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`.
pub unsafe extern "C" fn Bambu_Open(tunnel: *mut c_void) -> c_int {
    unsafe { tunnel_ref(tunnel) }.map_or(BAMBU_INVALID, Tunnel::open)
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`.
pub unsafe extern "C" fn Bambu_StartStream(tunnel: *mut c_void, video: bool) -> c_int {
    unsafe { tunnel_ref(tunnel) }.map_or(BAMBU_INVALID, |tunnel| tunnel.start_stream(video))
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_StartStreamEx(_tunnel: *mut c_void, _stream_type: c_int) -> c_int {
    BAMBU_STREAM_END
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`.
pub unsafe extern "C" fn Bambu_GetStreamCount(tunnel: *mut c_void) -> c_int {
    if unsafe { tunnel_ref(tunnel) }.is_some() {
        1
    } else {
        BAMBU_INVALID
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`; any non-null `info` must be writable.
pub unsafe extern "C" fn Bambu_GetStreamInfo(
    tunnel: *mut c_void,
    index: c_int,
    info: *mut BambuStreamInfo,
) -> c_int {
    unsafe { tunnel_ref(tunnel) }.map_or(BAMBU_INVALID, |tunnel| tunnel.stream_info(index, info))
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_GetDuration(_tunnel: *mut c_void) -> BambuDuration {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_Seek(_tunnel: *mut c_void, _time: c_ulong) -> c_int {
    BAMBU_STREAM_END
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`; any non-null `sample` must be writable.
pub unsafe extern "C" fn Bambu_ReadSample(tunnel: *mut c_void, sample: *mut BambuSample) -> c_int {
    unsafe { tunnel_ref(tunnel) }.map_or(BAMBU_INVALID, |tunnel| tunnel.read_sample(sample))
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_SendMessage(
    _tunnel: *mut c_void,
    _control: c_int,
    _data: *const c_char,
    _length: c_int,
) -> c_int {
    BAMBU_STREAM_END
}

#[unsafe(no_mangle)]
/// # Safety
/// If non-null, `length` must be writable.
pub unsafe extern "C" fn Bambu_RecvMessage(
    _tunnel: *mut c_void,
    _control: *mut c_int,
    _data: *mut c_char,
    length: *mut c_int,
) -> c_int {
    if !length.is_null() {
        unsafe { length.write(0) };
    }
    BAMBU_STREAM_END
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`.
pub unsafe extern "C" fn Bambu_Close(tunnel: *mut c_void) {
    if let Some(tunnel) = unsafe { tunnel_ref(tunnel) } {
        tunnel.close();
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a uniquely owned live handle returned by `Bambu_Create` and must
/// not be used again after this call.
pub unsafe extern "C" fn Bambu_Destroy(tunnel: *mut c_void) {
    if !tunnel.is_null() {
        let tunnel = unsafe { Box::from_raw(tunnel.cast::<Tunnel>()) };
        tunnel.close();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_Init() -> c_int {
    BAMBU_SUCCESS
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `tunnel` must be a live handle returned by `Bambu_Create`; any non-null `stat` must be writable.
pub unsafe extern "C" fn Bambu_GetSessionStat(tunnel: *mut c_void, stat: *mut BambuSessionStat) {
    if let Some(tunnel) = unsafe { tunnel_ref(tunnel) } {
        tunnel.session_stat(stat);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_Deinit() {}

#[unsafe(no_mangle)]
pub extern "C" fn Bambu_GetLastErrorMsg() -> *const c_char {
    last_error_message()
}

#[cfg(not(target_os = "windows"))]
#[unsafe(no_mangle)]
/// # Safety
/// `message` must be null or a pointer passed to the logger callback by this library exactly once.
pub unsafe extern "C" fn Bambu_FreeLogMsg(message: *const PlatformChar) {
    if !message.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(message.cast_mut()) });
    }
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
/// # Safety
/// `message` must be null or a pointer passed to the logger callback by this library exactly once.
pub unsafe extern "C" fn Bambu_FreeLogMsg(message: *const PlatformChar) {
    if message.is_null() {
        return;
    }
    let mut len = 0;
    while unsafe { *message.add(len) } != 0 {
        len += 1;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(message.cast_mut(), len + 1);
    drop(unsafe { Box::from_raw(slice) });
}

unsafe fn tunnel_ref(tunnel: *mut c_void) -> Option<&'static Tunnel> {
    unsafe { tunnel.cast::<Tunnel>().as_ref() }
}

#[cfg(test)]
mod tests;
