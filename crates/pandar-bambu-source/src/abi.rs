use std::ffi::{c_char, c_int, c_longlong, c_uchar, c_ulong, c_ulonglong, c_void};

pub(crate) const BAMBU_SUCCESS: c_int = 0;
pub(crate) const BAMBU_STREAM_END: c_int = 1;
pub(crate) const BAMBU_WOULD_BLOCK: c_int = 2;
pub(crate) const BAMBU_INVALID: c_int = -2;
pub(crate) const VIDEO_STREAM: c_int = 0;
pub(crate) const VIDEO_MJPG: c_int = 1;
pub(crate) const VIDEO_JPEG: c_int = 2;

#[cfg(target_os = "windows")]
pub type PlatformChar = u16;
#[cfg(not(target_os = "windows"))]
pub type PlatformChar = c_char;

pub type Logger = unsafe extern "C" fn(*mut c_void, c_int, *const PlatformChar);
pub type StreamInfoCallback = unsafe extern "C" fn(*mut c_void, *mut BambuStreamInfo);
pub type TrackReporter = unsafe extern "C" fn(*mut c_void, *const BambuPlayerEvent);

#[repr(C)]
pub struct BambuPlayerEvent {
    pub event_name: *const c_char,
    pub module: *const c_char,
    pub phase: *const c_char,
    pub result: *const c_char,
    pub error_code: *const c_char,
    pub error_message: *const c_char,
    pub event_data_body: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BambuVideoFormat {
    pub width: c_int,
    pub height: c_int,
    pub frame_rate: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BambuAudioFormat {
    pub sample_rate: c_int,
    pub channel_count: c_int,
    pub sample_size: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union BambuFormat {
    pub video: BambuVideoFormat,
    pub audio: BambuAudioFormat,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BambuStreamInfo {
    pub stream_type: c_int,
    pub sub_type: c_int,
    pub format: BambuFormat,
    pub format_type: c_int,
    pub format_size: c_int,
    pub max_frame_size: c_int,
    pub format_buffer: *const c_uchar,
}

#[repr(C)]
pub struct BambuSample {
    pub itrack: c_int,
    pub size: c_int,
    pub flags: c_int,
    pub buffer: *const c_uchar,
    pub decode_time: c_ulonglong,
}

#[repr(C)]
pub struct BambuSessionStat {
    pub session_duration_ms: c_longlong,
    pub freeze_total_duration_ms: c_longlong,
    pub freeze_count: c_int,
    pub avg_fps: f32,
    pub avg_bitrate_kbps: f32,
    pub avg_jitter_ms: f32,
    pub max_jitter_ms: f32,
}

pub type BambuDuration = c_ulong;
