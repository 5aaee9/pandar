use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use super::*;

extern "C" fn cancelled(context: *mut c_void) -> i32 {
    let cancelled = unsafe { &*context.cast::<AtomicBool>() };
    cancelled.load(Ordering::Acquire) as i32
}

fn key() -> NoAuthRotationKey {
    NoAuthRotationKey::new("http://hub".to_owned(), "old-token".to_owned(), 0, 7)
}

fn cancelled_outcome() -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 0,
        body: crate::stable_error_body("request_cancelled"),
    }
}

#[test]
fn cancelled_follower_leaves_the_owned_rotation_in_flight() {
    let session = Arc::new(ConnectionSession::new(
        "http://hub".to_owned(),
        "old-token".to_owned(),
    ));
    let rotation_key = key();
    assert_eq!(
        session.begin_no_auth_rotation(rotation_key.clone()),
        NoAuthRotationBegin::Started
    );

    let flag = Arc::new(AtomicBool::new(false));
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let waiter_session = Arc::clone(&session);
    let waiter_flag = Arc::clone(&flag);
    let waiter_key = rotation_key.clone();
    let waiter = thread::spawn(move || {
        let cancellation =
            RequestCancellation::new(Arc::as_ptr(&waiter_flag).cast_mut().cast(), Some(cancelled));
        waiter_session.begin_no_auth_rotation_before_wait(waiter_key, cancellation, || {
            waiting_tx.send(()).expect("announce follower wait")
        })
    });
    waiting_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("follower reached owned rotation");
    flag.store(true, Ordering::Release);

    assert_eq!(
        waiter.join().expect("follower result"),
        NoAuthRotationBegin::Cancelled
    );
    assert!(session.finish_no_auth_rotation(rotation_key.clone(), cancelled_outcome()));
    assert!(matches!(
        session.begin_no_auth_rotation(rotation_key),
        NoAuthRotationBegin::Finished(_)
    ));
}

#[test]
fn cancelled_owner_outcome_is_published_once_and_wakes_followers() {
    let session = Arc::new(ConnectionSession::new(
        "http://hub".to_owned(),
        "old-token".to_owned(),
    ));
    let rotation_key = key();
    let outcome = cancelled_outcome();
    assert_eq!(
        session.begin_no_auth_rotation(rotation_key.clone()),
        NoAuthRotationBegin::Started
    );

    let (waiting_tx, waiting_rx) = mpsc::channel();
    let waiter_session = Arc::clone(&session);
    let waiter_key = rotation_key.clone();
    let waiter = thread::spawn(move || {
        waiter_session.begin_no_auth_rotation_before_wait(
            waiter_key,
            RequestCancellation::disabled(),
            || waiting_tx.send(()).expect("announce follower wait"),
        )
    });
    waiting_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("follower reached owned rotation");

    assert!(session.finish_no_auth_rotation(rotation_key.clone(), outcome.clone()));
    assert!(!session.finish_no_auth_rotation(rotation_key, outcome.clone()));
    assert_eq!(
        waiter.join().expect("follower result"),
        NoAuthRotationBegin::Finished(outcome)
    );
}
