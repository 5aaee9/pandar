use std::{
    sync::{Arc, mpsc},
    thread,
};

use super::*;

fn session() -> Arc<ConnectionSession> {
    Arc::new(ConnectionSession::new(
        "http://127.0.0.1:8080".to_owned(),
        "token".to_owned(),
    ))
}

fn failure_outcome() -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 503,
        body: r#"{"error":"invalid_response"}"#.to_owned(),
    }
}

#[test]
fn external_requested_follower_upgrades_passive_owner_and_reuses_exact_outcome() {
    let session = session();
    let mut owner = match session.begin_account_logout(false) {
        AccountLogoutBegin::Owner(owner) => owner,
        _ => panic!("passive logout did not become owner"),
    };
    let (joined_tx, joined_rx) = mpsc::channel();
    let follower_session = Arc::clone(&session);
    let follower = thread::spawn(move || match follower_session.begin_account_logout(true) {
        AccountLogoutBegin::Follower(follower) => {
            joined_tx.send(()).unwrap();
            follower.wait()
        }
        _ => panic!("requested logout did not join the passive owner"),
    });

    joined_rx.recv().unwrap();
    owner.begin_finalization();
    assert!(owner.seal_finalization());
    let expected = failure_outcome();
    owner.complete(true, expected.clone());
    assert_eq!(follower.join().unwrap(), expected);
}

#[test]
fn requested_cas_either_upgrades_owner_or_starts_after_passive_finalization() {
    let session = session();
    let mut passive = match session.begin_account_logout(false) {
        AccountLogoutBegin::Owner(owner) => owner,
        _ => panic!("passive logout did not become owner"),
    };
    passive.begin_finalization();
    assert!(!passive.seal_finalization());
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let requested_session = Arc::clone(&session);
    let requested = thread::spawn(move || {
        let mut requested = match requested_session.begin_account_logout(true) {
            AccountLogoutBegin::Owner(owner) => owner,
            _ => panic!("requested logout did not become the next owner"),
        };
        acquired_tx.send(()).unwrap();
        requested.begin_finalization();
        assert!(requested.seal_finalization());
        requested.complete(
            true,
            NoAuthRotationOutcome {
                status: 0,
                http_code: 204,
                body: String::new(),
            },
        );
    });
    session.wait_for_account_logout_committed_waiter();
    assert!(matches!(
        acquired_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    passive.complete(
        false,
        NoAuthRotationOutcome {
            status: 0,
            http_code: 204,
            body: String::new(),
        },
    );
    acquired_rx.recv().unwrap();
    requested.join().unwrap();
}

#[test]
fn external_requested_follower_upgrades_passive_finalizing_before_seal() {
    let session = session();
    let mut owner = match session.begin_account_logout(false) {
        AccountLogoutBegin::Owner(owner) => owner,
        _ => panic!("passive logout did not become owner"),
    };
    owner.begin_finalization();

    let (joined_tx, joined_rx) = mpsc::channel();
    let follower_session = Arc::clone(&session);
    let follower = thread::spawn(move || match follower_session.begin_account_logout(true) {
        AccountLogoutBegin::Follower(follower) => {
            joined_tx.send(()).unwrap();
            follower.wait()
        }
        _ => panic!("requested logout did not upgrade passive finalization"),
    });

    joined_rx.recv().unwrap();
    assert!(owner.seal_finalization());
    let expected = failure_outcome();
    owner.complete(true, expected.clone());
    assert_eq!(follower.join().unwrap(), expected);
}

#[test]
fn requested_committed_follower_reuses_the_exact_owner_outcome() {
    let session = session();
    let mut owner = match session.begin_account_logout(true) {
        AccountLogoutBegin::Owner(owner) => owner,
        _ => panic!("requested logout did not become owner"),
    };
    owner.begin_finalization();
    assert!(owner.seal_finalization());

    let (joined_tx, joined_rx) = mpsc::channel();
    let follower_session = Arc::clone(&session);
    let follower = thread::spawn(move || match follower_session.begin_account_logout(true) {
        AccountLogoutBegin::Follower(follower) => {
            joined_tx.send(()).unwrap();
            follower.wait()
        }
        _ => panic!("requested logout did not join committed owner"),
    });

    joined_rx.recv().unwrap();
    let expected = failure_outcome();
    owner.complete(true, expected.clone());
    assert_eq!(follower.join().unwrap(), expected);
}

#[test]
fn dropping_upgraded_finalizing_owner_wakes_follower_with_stable_failure() {
    let session = session();
    let mut owner = match session.begin_account_logout(false) {
        AccountLogoutBegin::Owner(owner) => owner,
        _ => panic!("passive logout did not become owner"),
    };
    owner.begin_finalization();

    let (joined_tx, joined_rx) = mpsc::channel();
    let follower_session = Arc::clone(&session);
    let follower = thread::spawn(move || match follower_session.begin_account_logout(true) {
        AccountLogoutBegin::Follower(follower) => {
            joined_tx.send(()).unwrap();
            follower.wait()
        }
        _ => panic!("requested logout did not upgrade finalizing owner"),
    });

    joined_rx.recv().unwrap();
    drop(owner);
    let outcome = follower.join().unwrap();
    assert_eq!(outcome.status, 1);
    assert_eq!(outcome.http_code, 0);
    assert_eq!(
        outcome.body,
        crate::stable_error_body("account_state_unavailable")
    );
}
