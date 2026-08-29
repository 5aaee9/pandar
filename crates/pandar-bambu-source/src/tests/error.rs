use super::*;

#[test]
fn handshake_failure_preserves_the_write_cause() {
    struct FailingHandshake;

    impl Write for FailingHandshake {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "handshake write cause sentinel",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let error = send_relay_handshake(&mut FailingHandshake, &[b'a'; 32]).unwrap_err();
    assert_eq!(
        crate::error::error_chain(&error),
        "local camera relay handshake failed: handshake write cause sentinel"
    );
}

#[test]
fn read_failure_is_logged_and_distinct_from_eof() {
    let _guard = ERROR_TEST_LOCK.lock().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "1123456789abcdef0123456789abcdef";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        stream.write_all(&[12, 0]).unwrap();
    });
    let logs = Mutex::new(Vec::new());
    let tunnel = create_tunnel(&relay_url(port, auth), &logs);

    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
    assert_eq!(wait_for_sample_result(tunnel), BAMBU_INVALID);
    let message = logs.lock().unwrap().join("\n");
    assert!(message.contains("local camera stream failed while reading a frame length"));
    assert!(message.contains("failed to fill whole buffer"));
    let last_error = unsafe { CStr::from_ptr(Bambu_GetLastErrorMsg()) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(last_error, message);

    unsafe { Bambu_Destroy(tunnel) };
    server.join().unwrap();
}

#[test]
fn clean_relay_eof_remains_a_normal_stream_end() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "2123456789abcdef0123456789abcdef";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
    });
    let logs = Mutex::new(Vec::new());
    let tunnel = create_tunnel(&relay_url(port, auth), &logs);

    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
    assert_eq!(wait_for_sample_result(tunnel), BAMBU_STREAM_END);
    assert!(logs.lock().unwrap().is_empty());

    unsafe { Bambu_Destroy(tunnel) };
    server.join().unwrap();
}

#[test]
fn later_session_failure_replaces_the_current_thread_last_error() {
    let _guard = ERROR_TEST_LOCK.lock().unwrap();
    let first_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let first_port = first_listener.local_addr().unwrap().port();
    let auth = "3123456789abcdef0123456789abcdef";
    let first_server = std::thread::spawn(move || {
        let (mut stream, _) = first_listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        stream.write_all(&[4, 0, 0, 0, 0xff]).unwrap();
    });
    let first_logs = Mutex::new(Vec::new());
    let first = create_tunnel(&relay_url(first_port, auth), &first_logs);
    assert_eq!(unsafe { Bambu_Open(first) }, BAMBU_SUCCESS);
    assert_eq!(wait_for_sample_result(first), BAMBU_INVALID);
    let first_text = unsafe { CStr::from_ptr(Bambu_GetLastErrorMsg()) }
        .to_string_lossy()
        .into_owned();
    assert!(first_text.contains("reading a frame body"));

    let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
    let unavailable_port = unavailable.local_addr().unwrap().port();
    drop(unavailable);
    let second_logs = Mutex::new(Vec::new());
    let second = create_tunnel(&relay_url(unavailable_port, auth), &second_logs);
    assert_eq!(unsafe { Bambu_Open(second) }, BAMBU_INVALID);

    let second_text = unsafe { CStr::from_ptr(Bambu_GetLastErrorMsg()) }
        .to_string_lossy()
        .into_owned();
    assert!(second_text.contains("local camera transport failed while connecting"));
    assert_ne!(second_text, first_text);
    assert_eq!(second_logs.lock().unwrap().as_slice(), &[second_text]);

    unsafe {
        Bambu_Destroy(first);
        Bambu_Destroy(second);
    }
    first_server.join().unwrap();
}

#[test]
fn logger_message_remains_owned_until_studio_frees_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "4123456789abcdef0123456789abcdef";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        stream.write_all(&0_u32.to_le_bytes()).unwrap();
    });
    let retained: Mutex<Option<usize>> = Mutex::new(None);
    let mut tunnel = ptr::null_mut();
    let url = relay_url(port, auth);
    assert_eq!(
        unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) },
        BAMBU_SUCCESS
    );
    unsafe {
        Bambu_SetLogger(
            tunnel,
            Some(retain_log_pointer),
            std::ptr::from_ref(&retained).cast_mut().cast(),
        );
    }

    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
    assert_eq!(wait_for_sample_result(tunnel), BAMBU_INVALID);
    let pointer = retained.lock().unwrap().take().unwrap() as *const PlatformChar;
    #[cfg(not(target_os = "windows"))]
    let message = unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned();
    #[cfg(target_os = "windows")]
    let message = {
        let mut length = 0;
        while unsafe { *pointer.add(length) } != 0 {
            length += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
    };
    assert!(message.contains("invalid frame length 0"));
    unsafe {
        Bambu_FreeLogMsg(pointer);
        Bambu_Destroy(tunnel);
    }
    server.join().unwrap();
}

#[test]
fn concurrent_tunnels_keep_last_errors_thread_local() {
    let barrier = Arc::new(Barrier::new(2));
    let run = |length: u32, auth: &'static str, barrier: Arc<Barrier>| {
        std::thread::spawn(move || {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut presented = [0_u8; 32];
                stream.read_exact(&mut presented).unwrap();
                stream.write_all(&length.to_le_bytes()).unwrap();
            });
            let logs = Mutex::new(Vec::new());
            let tunnel = create_tunnel(&relay_url(port, auth), &logs);
            assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
            assert_eq!(wait_for_sample_result(tunnel), BAMBU_INVALID);
            barrier.wait();
            let error = unsafe { CStr::from_ptr(Bambu_GetLastErrorMsg()) }
                .to_string_lossy()
                .into_owned();
            unsafe { Bambu_Destroy(tunnel) };
            server.join().unwrap();
            error
        })
    };
    let first = run(0, "5123456789abcdef0123456789abcdef", Arc::clone(&barrier));
    let second = run(
        (crate::tunnel::MAX_FRAME_BYTES + 1) as u32,
        "6123456789abcdef0123456789abcdef",
        barrier,
    );

    assert!(first.join().unwrap().contains("invalid frame length 0"));
    assert!(second.join().unwrap().contains(&format!(
        "invalid frame length {}",
        crate::tunnel::MAX_FRAME_BYTES + 1
    )));
}

struct BlockingLogger {
    entered: Barrier,
    release: Barrier,
}

unsafe extern "C" fn block_log_until_released(
    context: *mut c_void,
    _level: i32,
    message: *const PlatformChar,
) {
    let state = unsafe { &*context.cast::<BlockingLogger>() };
    state.entered.wait();
    state.release.wait();
    unsafe { Bambu_FreeLogMsg(message) };
}

#[test]
fn replacing_logger_waits_for_the_in_flight_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth = "7123456789abcdef0123456789abcdef";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut presented = [0_u8; 32];
        stream.read_exact(&mut presented).unwrap();
        stream.write_all(&0_u32.to_le_bytes()).unwrap();
    });
    let state = BlockingLogger {
        entered: Barrier::new(2),
        release: Barrier::new(2),
    };
    let mut tunnel = ptr::null_mut();
    let url = relay_url(port, auth);
    assert_eq!(
        unsafe { Bambu_Create(&mut tunnel, url.as_ptr()) },
        BAMBU_SUCCESS
    );
    unsafe {
        Bambu_SetLogger(
            tunnel,
            Some(block_log_until_released),
            std::ptr::from_ref(&state).cast_mut().cast(),
        );
    }
    assert_eq!(unsafe { Bambu_Open(tunnel) }, BAMBU_SUCCESS);
    state.entered.wait();

    let (replaced, replacement) = std::sync::mpsc::sync_channel(1);
    let tunnel_address = tunnel as usize;
    let replacing = std::thread::spawn(move || {
        unsafe {
            Bambu_SetLogger(tunnel_address as *mut c_void, None, std::ptr::null_mut());
        }
        replaced.send(()).unwrap();
    });
    assert!(replacement.recv_timeout(Duration::from_millis(50)).is_err());

    state.release.wait();
    replacement.recv_timeout(Duration::from_secs(1)).unwrap();
    replacing.join().unwrap();
    assert_eq!(wait_for_sample_result(tunnel), BAMBU_INVALID);
    unsafe { Bambu_Destroy(tunnel) };
    server.join().unwrap();
}
