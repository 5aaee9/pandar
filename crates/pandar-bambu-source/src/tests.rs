use std::{
    ffi::{CString, c_void},
    io::{Read, Write},
    net::TcpListener,
    ptr,
    time::{Duration, Instant},
};

use super::*;
use crate::abi::{BAMBU_WOULD_BLOCK, VIDEO_JPEG, VIDEO_MJPG, VIDEO_STREAM};

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
fn jpeg_dimensions_are_read_from_sof() {
    let jpeg = [
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x04, 0x38, 0x07, 0x80, 0xff, 0xd9,
    ];
    assert_eq!(crate::config::jpeg_dimensions(&jpeg), Some((1920, 1080)));
    assert_eq!(crate::config::jpeg_dimensions(b"not-jpeg"), None);
}
