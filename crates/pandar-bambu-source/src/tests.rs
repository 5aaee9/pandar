use std::{
    ffi::{CStr, CString, c_void},
    io::{self, Read, Write},
    net::TcpListener,
    ptr,
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use super::*;
use crate::{
    abi::{BAMBU_WOULD_BLOCK, PlatformChar, VIDEO_JPEG, VIDEO_MJPG, VIDEO_STREAM},
    tunnel::send_relay_handshake,
};

mod error;

static ERROR_TEST_LOCK: Mutex<()> = Mutex::new(());

unsafe extern "C" fn collect_log(context: *mut c_void, _level: i32, message: *const PlatformChar) {
    #[cfg(not(target_os = "windows"))]
    let text = unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned();
    #[cfg(target_os = "windows")]
    let text = {
        let mut length = 0;
        while unsafe { *message.add(length) } != 0 {
            length += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(message, length) })
    };
    unsafe { &*(context.cast::<Mutex<Vec<String>>>()) }
        .lock()
        .unwrap()
        .push(text);
    unsafe { Bambu_FreeLogMsg(message) };
}

struct ReentrantCloseContext {
    tunnel: *mut c_void,
    returned: AtomicBool,
}

unsafe extern "C" fn close_from_stream_info(context: *mut c_void, _info: *mut BambuStreamInfo) {
    let context = unsafe { &*(context.cast::<ReentrantCloseContext>()) };
    unsafe { Bambu_Close(context.tunnel) };
    context.returned.store(true, Ordering::Release);
}

unsafe extern "C" fn retain_log_pointer(
    context: *mut c_void,
    _level: i32,
    message: *const PlatformChar,
) {
    *unsafe { &*(context.cast::<Mutex<Option<usize>>>()) }
        .lock()
        .unwrap() = Some(message as usize);
}

fn relay_url(port: u16, auth: &str) -> CString {
    CString::new(format!(
        "bambu:///local/127.0.0.1?port={port}&auth={auth}&device=SERIAL"
    ))
    .unwrap()
}

fn create_tunnel(url: &CString, logs: &Mutex<Vec<String>>) -> *mut c_void {
    let mut tunnel = ptr::null_mut();
    assert_eq!(
        unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) },
        BAMBU_SUCCESS
    );
    unsafe {
        Bambu_SetLogger(
            tunnel,
            Some(collect_log),
            std::ptr::from_ref(logs).cast_mut().cast(),
        );
    }
    tunnel
}

fn wait_for_sample_result(tunnel: *mut c_void) -> i32 {
    let mut sample = std::mem::MaybeUninit::<BambuSample>::uninit();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let result = unsafe { Bambu_ReadSample(tunnel, sample.as_mut_ptr()) };
        if result != BAMBU_WOULD_BLOCK {
            return result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn sentinel_identifies_the_local_camera_source() {
    assert_eq!(pandar_bambu_source_sentinel(), 1);
}

#[test]
fn media_struct_layout_matches_pinned_studio() {
    assert_eq!(std::mem::offset_of!(BambuStreamInfo, stream_type), 0);
    assert_eq!(std::mem::offset_of!(BambuStreamInfo, sub_type), 4);
    assert_eq!(std::mem::offset_of!(BambuStreamInfo, format), 8);
    assert_eq!(std::mem::offset_of!(BambuStreamInfo, format_type), 20);
    assert_eq!(std::mem::offset_of!(BambuStreamInfo, format_buffer), 32);
    assert_eq!(
        std::mem::size_of::<BambuStreamInfo>(),
        if cfg!(target_pointer_width = "64") {
            40
        } else {
            36
        }
    );
    assert_eq!(std::mem::offset_of!(BambuSample, itrack), 0);
    assert_eq!(
        std::mem::offset_of!(BambuSample, buffer),
        if cfg!(target_pointer_width = "64") {
            16
        } else {
            12
        }
    );
    assert_eq!(
        std::mem::size_of::<BambuSample>(),
        if cfg!(target_pointer_width = "64") {
            32
        } else {
            24
        }
    );
}

#[test]
fn rejects_nonlocal_and_incomplete_relay_urls() {
    for url in [
        "https://hub.example.test/camera",
        "bambu:///rtsps___bblp:secret@printer/streaming/live/1",
        "bambu:///local/127.0.0.1?port=1234",
        "bambu:///local/127.0.0.1?port=1234&auth=short",
    ] {
        let url = CString::new(url).unwrap();
        let mut tunnel = ptr::null_mut();
        let result = unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) };
        assert_eq!(result, BAMBU_INVALID, "{url:?}");
        assert!(tunnel.is_null());
    }
}

#[test]
fn local_relay_yields_mjpeg_samples_through_the_studio_abi() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "0123456789abcdef0123456789abcdef";
    let jpeg = vec![
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x01, 0xe0, 0x02, 0x80, 0xff, 0xd9,
    ];
    let expected_jpeg = jpeg.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        assert_eq!(&presented, auth.as_bytes());
        stream
            .write_all(&(jpeg.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(&jpeg).unwrap();
        std::thread::sleep(Duration::from_millis(200));
    });
    let url = CString::new(format!(
        "bambu:///local/127.0.0.1?port={port}&auth={auth}&device=SERIAL"
    ))
    .unwrap();
    let mut tunnel: *mut c_void = ptr::null_mut();

    assert_eq!(
        unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) },
        BAMBU_SUCCESS
    );
    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let result = unsafe { Bambu_StartStream(tunnel, true) };
        if result == BAMBU_SUCCESS {
            break;
        }
        assert_eq!(result, BAMBU_WOULD_BLOCK);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut info = std::mem::MaybeUninit::<BambuStreamInfo>::uninit();
    assert_eq!(
        unsafe { Bambu_GetStreamInfo(tunnel, 0, info.as_mut_ptr()) },
        BAMBU_SUCCESS
    );
    let info = unsafe { info.assume_init() };
    let video = unsafe { info.format.video };
    assert_eq!(info.stream_type, VIDEO_STREAM);
    assert_eq!(info.sub_type, VIDEO_MJPG);
    assert_eq!(info.format_type, VIDEO_JPEG);
    assert_eq!((video.width, video.height, video.frame_rate), (640, 480, 5));

    let mut sample = std::mem::MaybeUninit::<BambuSample>::uninit();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let result = unsafe { Bambu_ReadSample(tunnel, sample.as_mut_ptr()) };
        if result == BAMBU_SUCCESS {
            break;
        }
        assert_eq!(result, BAMBU_WOULD_BLOCK);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let sample = unsafe { sample.assume_init() };
    let body = unsafe { std::slice::from_raw_parts(sample.buffer, sample.size as usize) };
    assert_eq!(body, expected_jpeg);

    let mut stat = std::mem::MaybeUninit::<BambuSessionStat>::uninit();
    unsafe { Bambu_GetSessionStat(tunnel, stat.as_mut_ptr()) };
    assert!(unsafe { stat.assume_init() }.avg_bitrate_kbps >= 0.0);
    unsafe {
        Bambu_Close(tunnel);
        Bambu_Destroy(tunnel);
    }
    server.join().unwrap();
}

#[test]
fn stream_info_callback_can_close_its_own_reader_worker() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "0123456789abcdef0123456789abcdef";
    let jpeg = vec![
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x01, 0xe0, 0x02, 0x80, 0xff, 0xd9,
    ];
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        assert_eq!(&presented, auth.as_bytes());
        stream
            .write_all(&(jpeg.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(&jpeg).unwrap();
    });
    let url = CString::new(format!(
        "bambu:///local/127.0.0.1?port={port}&auth={auth}&device=SERIAL"
    ))
    .unwrap();
    let mut tunnel: *mut c_void = ptr::null_mut();
    assert_eq!(
        unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) },
        BAMBU_SUCCESS
    );
    let context = Box::new(ReentrantCloseContext {
        tunnel,
        returned: AtomicBool::new(false),
    });
    unsafe {
        Bambu_SetStreamInfoCallback(
            tunnel,
            Some(close_from_stream_info),
            std::ptr::from_ref(&*context).cast_mut().cast(),
        );
    }
    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);

    let deadline = Instant::now() + Duration::from_secs(2);
    while !context.returned.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(context.returned.load(Ordering::Acquire));
    while unsafe { Bambu_StartStream(tunnel, true) } != BAMBU_INVALID && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(unsafe { Bambu_StartStream(tunnel, true) }, BAMBU_INVALID);
    unsafe { Bambu_Destroy(tunnel) };
    server.join().unwrap();
}

#[test]
fn jpeg_dimensions_are_read_from_sof() {
    let jpeg = [
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x04, 0x38, 0x07, 0x80, 0xff, 0xd9,
    ];
    assert_eq!(crate::config::jpeg_dimensions(&jpeg), Some((1920, 1080)));
    assert_eq!(crate::config::jpeg_dimensions(b"not-jpeg"), None);
}
