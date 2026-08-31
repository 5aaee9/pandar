use std::{
    ffi::c_void,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::http::EmptyRequest;

extern "C" fn cancelled(context: *mut c_void) -> i32 {
    let cancelled = unsafe { &*context.cast::<AtomicBool>() };
    cancelled.load(Ordering::Acquire) as i32
}

fn cancellation(flag: &Arc<AtomicBool>) -> RequestCancellation {
    RequestCancellation::new(Arc::as_ptr(flag).cast_mut().cast(), Some(cancelled))
}

fn assert_cancelled(result: PluginHttpResult) {
    let body = String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(result.body_ptr, result.body_len)
    })
    .into_owned();
    unsafe {
        crate::pandar_plugin_free_with_capacity(
            result.body_ptr.cast(),
            result.body_len,
            result.body_cap,
        )
    };
    assert_eq!(result.status, 1);
    assert_eq!(result.http_code, 0);
    assert_eq!(body, crate::stable_error_body("request_cancelled"));
}

#[test]
fn no_auth_post_cancels_while_the_response_body_is_pending() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind no-auth body server");
    let url = format!(
        "http://{}/api/v1/plugin/no-auth-session",
        listener.local_addr().unwrap()
    );
    let (pending_tx, pending_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept no-auth POST");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("read no-auth POST");
        assert!(read > 0, "no-auth POST was empty");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection: close\r\n\r\n")
            .expect("write pending response headers");
        pending_tx.send(()).expect("announce pending response body");
        thread::sleep(Duration::from_secs(3));
    });
    let flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::clone(&flag);
    let canceller = thread::spawn(move || {
        pending_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("response body became pending");
        cancel_flag.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let result = post_json_with_connect_failure(
        &url,
        EmptyRequest {},
        RequestKind::TicketExchange,
        cancellation(&flag),
    );
    let elapsed = started.elapsed();
    assert_cancelled(result);
    assert!(
        elapsed < Duration::from_secs(2),
        "POST cancellation took {elapsed:?}"
    );
    canceller.join().expect("no-auth POST canceller");
    server.join().expect("no-auth body server");
}

#[test]
fn pending_revocation_delete_cancels_while_waiting_for_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind revocation server");
    let url = format!(
        "http://{}/api/v1/plugin/session",
        listener.local_addr().unwrap()
    );
    let (pending_tx, pending_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept revocation DELETE");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("read revocation DELETE");
        assert!(read > 0, "revocation DELETE was empty");
        pending_tx.send(()).expect("announce pending DELETE");
        thread::sleep(Duration::from_secs(3));
    });
    let flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::clone(&flag);
    let canceller = thread::spawn(move || {
        pending_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("DELETE became pending");
        cancel_flag.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let result = delete_session(
        &url,
        "stale-token",
        RequestKind::PluginSession,
        cancellation(&flag),
    );
    let elapsed = started.elapsed();
    assert_cancelled(result);
    assert!(
        elapsed < Duration::from_secs(2),
        "DELETE cancellation took {elapsed:?}"
    );
    canceller.join().expect("revocation DELETE canceller");
    server.join().expect("revocation server");
}
