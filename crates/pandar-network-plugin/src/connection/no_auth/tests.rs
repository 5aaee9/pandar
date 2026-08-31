use std::{
    sync::{Arc, Barrier},
    thread,
};

use crate::{
    pandar_plugin_connection_set_account_epoch, pandar_plugin_no_auth_retryable_connect_failure,
    pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_update,
};

use super::{
    pandar_plugin_no_auth_retry_active, pandar_plugin_no_auth_retry_arm,
    pandar_plugin_no_auth_retry_begin, pandar_plugin_no_auth_retry_complete,
};

fn session() -> *mut std::ffi::c_void {
    let hub_url = "http://127.0.0.1:8080";
    let token = "";
    unsafe {
        pandar_plugin_printer_refresh_session_create(
            hub_url.as_ptr(),
            hub_url.len(),
            token.as_ptr(),
            token.len(),
        )
    }
}

#[test]
fn no_auth_retry_active_only_while_waiting_or_in_flight() {
    let session = session();
    assert!(!session.is_null());
    assert!(!unsafe { pandar_plugin_no_auth_retry_active(session) });

    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(session, 1_000) },
        0
    );
    assert!(unsafe { pandar_plugin_no_auth_retry_active(session) });
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, 1_000) },
        1
    );
    assert!(unsafe { pandar_plugin_no_auth_retry_active(session) });
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(session, 0, 1_000) },
        0
    );
    assert!(!unsafe { pandar_plugin_no_auth_retry_active(session) });

    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn connect_failure_leaves_no_auth_retry_active_while_waiting() {
    let session = session();
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(session, 1_000) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, 1_000) },
        1
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(session, 2, 1_000) },
        0
    );
    assert!(unsafe { pandar_plugin_no_auth_retry_active(session) });

    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn no_auth_retry_claims_one_attempt_and_only_rearms_after_connect_failure_delay() {
    let session = session();
    assert!(!session.is_null());
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(session, 1_000) },
        0
    );

    let barrier = Arc::new(Barrier::new(33));
    let attempts = (0..32)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let session = session as usize;
            thread::spawn(move || {
                barrier.wait();
                unsafe { pandar_plugin_no_auth_retry_begin(session as *mut _, 1_000) }
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    assert_eq!(
        attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .sum::<i32>(),
        1
    );

    let connect_failure_status = 2;
    assert!(pandar_plugin_no_auth_retryable_connect_failure(
        connect_failure_status
    ));
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(session, connect_failure_status, 1_000) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, 2_999) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, 3_000) },
        1
    );

    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(session, 1, 3_000) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(session, 3_000) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, u64::MAX) },
        0
    );

    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn no_auth_retry_stops_after_five_connect_failures() {
    let session = session();
    assert!(!session.is_null());
    assert_eq!(unsafe { pandar_plugin_no_auth_retry_arm(session, 0) }, 0);

    for _ in 0..5 {
        assert_eq!(
            unsafe { pandar_plugin_no_auth_retry_begin(session, u64::MAX) },
            1
        );
        assert_eq!(
            unsafe { pandar_plugin_no_auth_retry_complete(session, 2, u64::MAX) },
            0
        );
    }

    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, u64::MAX) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(session, u64::MAX) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(session, u64::MAX) },
        0
    );

    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn no_auth_retry_is_fenced_by_token_account_and_hub_changes() {
    let connect_failure_status = 2;

    let token_session = session();
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(token_session, 0) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(token_session, 0) },
        1
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(token_session, connect_failure_status, 0) },
        0
    );
    let hub_url = "http://127.0.0.1:8080";
    let token = "new-token";
    assert_eq!(
        unsafe {
            pandar_plugin_printer_refresh_session_update(
                token_session,
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(token_session, u64::MAX) },
        0
    );
    unsafe { pandar_plugin_printer_refresh_session_destroy(token_session) };

    let account_session = session();
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(account_session, 0) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(account_session, 0) },
        1
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(account_session, connect_failure_status, 0) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_connection_set_account_epoch(account_session, 1) },
        0
    );
    assert!(!unsafe { pandar_plugin_no_auth_retry_active(account_session) });
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(account_session, u64::MAX) },
        0
    );
    unsafe { pandar_plugin_printer_refresh_session_destroy(account_session) };

    let hub_session = session();
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_arm(hub_session, 0) },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(hub_session, 0) },
        1
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_complete(hub_session, connect_failure_status, 0) },
        0
    );
    let new_hub_url = "http://127.0.0.1:8081";
    let empty_token = "";
    assert_eq!(
        unsafe {
            pandar_plugin_printer_refresh_session_update(
                hub_session,
                new_hub_url.as_ptr(),
                new_hub_url.len(),
                empty_token.as_ptr(),
                empty_token.len(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { pandar_plugin_no_auth_retry_begin(hub_session, u64::MAX) },
        0
    );
    unsafe { pandar_plugin_printer_refresh_session_destroy(hub_session) };
}
