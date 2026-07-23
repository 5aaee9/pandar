use std::{
    sync::{Arc, Barrier, TryLockError, mpsc},
    time::Duration,
};

use super::{
    LOGIN_FILE, LOGIN_FILE_LOCK, MutationDurability, clear, clear_matching, complete_direct,
    complete_pending,
    durable::{FaultPoint, fail_next},
    enqueue_pending, load, load_after_login_lock, load_direct, load_pending, prepare_direct,
    prepare_orphan_direct, remove_pending, store,
};
use crate::account::types::{PendingRevocation, PersistedLogin, Profile, SessionKind};

fn login(token: &str) -> PersistedLogin {
    PersistedLogin {
        hub_url: "http://127.0.0.1:18080".to_owned(),
        token: token.to_owned(),
        session_kind: SessionKind::NoAuth,
        profile: Profile {
            user_id: "user-1".to_owned(),
            user_name: "User One".to_owned(),
            tenant_id: "tenant-1".to_owned(),
            tenant_name: "Tenant One".to_owned(),
            avatar: String::new(),
        },
    }
}

#[test]
fn pending_revocation_hides_and_clears_only_its_matching_login() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let login = login("current-token");
    store(&config_dir, &login).unwrap();

    let unrelated = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: "old-token".to_owned(),
    };
    enqueue_pending(&config_dir, unrelated.clone()).unwrap();
    assert_eq!(load(&config_dir).unwrap(), Some(login.clone()));
    clear_matching(&config_dir, &unrelated).unwrap();
    assert!(directory.path().join(LOGIN_FILE).is_file());

    let matching = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: login.token.clone(),
    };
    enqueue_pending(&config_dir, matching.clone()).unwrap();
    assert_eq!(load(&config_dir).unwrap(), None);
    clear_matching(&config_dir, &matching).unwrap();
    assert!(!directory.path().join(LOGIN_FILE).exists());
}

#[test]
fn direct_revocation_intent_hides_login_until_completed() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let login = login("direct-token");
    let candidate = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: login.token.clone(),
    };
    store(&config_dir, &login).unwrap();

    prepare_direct(&config_dir, &candidate).unwrap();

    assert_eq!(load_direct(&config_dir).unwrap(), Some(candidate.clone()));
    assert_eq!(load(&config_dir).unwrap(), None);
    complete_direct(&config_dir, &candidate).unwrap();
    assert_eq!(load_direct(&config_dir).unwrap(), None);
    assert_eq!(load(&config_dir).unwrap(), None);
}

#[test]
fn orphan_revocation_intent_is_replayable_without_a_matching_login() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let candidate = PendingRevocation {
        hub_url: "http://127.0.0.1:18080".to_owned(),
        token: "orphan-token".to_owned(),
    };

    prepare_orphan_direct(&config_dir, &candidate).unwrap();

    assert_eq!(load_direct(&config_dir).unwrap(), Some(candidate.clone()));
    complete_direct(&config_dir, &candidate).unwrap();
    assert_eq!(load_direct(&config_dir).unwrap(), None);
}

#[test]
fn unconfirmed_direct_intent_is_visible_but_not_safe_to_send() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let login = login("uncertain-direct-token");
    let candidate = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: login.token.clone(),
    };
    store(&config_dir, &login).unwrap();
    fail_next(&[FaultPoint::WritePublish]);

    let outcome = prepare_direct(&config_dir, &candidate).unwrap();

    assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
    assert_eq!(load(&config_dir).unwrap(), None);
    assert_eq!(load_direct(&config_dir).unwrap(), Some(candidate));
}

#[test]
fn direct_intent_replay_waits_for_directory_durability_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let login = login("replay-confirmation-token");
    let candidate = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: login.token.clone(),
    };
    store(&config_dir, &login).unwrap();
    prepare_direct(&config_dir, &candidate).unwrap();
    fail_next(&[FaultPoint::WritePublish]);

    let error = load_direct(&config_dir).unwrap_err();

    assert!(format!("{error:#}").contains("confirm direct Studio revocation before replay"));
    assert_eq!(load(&config_dir).unwrap(), None);
}

#[test]
fn direct_revocation_completion_preserves_a_replacement_login() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let original = login("direct-token");
    let replacement = login("replacement-token");
    let candidate = PendingRevocation {
        hub_url: original.hub_url.clone(),
        token: original.token.clone(),
    };
    store(&config_dir, &original).unwrap();
    prepare_direct(&config_dir, &candidate).unwrap();
    enqueue_pending(&config_dir, candidate.clone()).unwrap();
    store(&config_dir, &replacement).unwrap();

    complete_direct(&config_dir, &candidate).unwrap();
    enqueue_pending(&config_dir, candidate.clone()).unwrap();
    complete_direct(&config_dir, &candidate).unwrap();

    assert_eq!(load_direct(&config_dir).unwrap(), None);
    assert!(load_pending(&config_dir).unwrap().is_empty());
    assert_eq!(load(&config_dir).unwrap(), Some(replacement));
}

#[test]
fn completed_revocation_blocks_a_stale_process_from_rewriting_the_token() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let original = login("completed-token");
    let replacement = login("replacement-token");
    let candidate = PendingRevocation {
        hub_url: original.hub_url.clone(),
        token: original.token.clone(),
    };
    store(&config_dir, &original).unwrap();
    enqueue_pending(&config_dir, candidate.clone()).unwrap();

    complete_pending(&config_dir, &candidate).unwrap();

    assert_eq!(load(&config_dir).unwrap(), None);
    assert!(load_pending(&config_dir).unwrap().is_empty());
    let error = store(&config_dir, &original).unwrap_err();
    assert!(error.to_string().contains("revoked Studio login"));
    store(&config_dir, &replacement).unwrap();
    assert_eq!(load(&config_dir).unwrap(), Some(replacement));
    let completed = std::fs::read_to_string(
        directory
            .path()
            .join("pandar-plugin-completed-revocations.json"),
    )
    .unwrap();
    assert!(!completed.contains("completed-token"));
}

#[test]
fn pending_write_reconfirms_a_publish_sync_failure() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let candidate = PendingRevocation {
        hub_url: "http://127.0.0.1:18080".to_owned(),
        token: "uncertain-pending-token".to_owned(),
    };
    fail_next(&[FaultPoint::WritePublish]);

    let outcome = enqueue_pending(&config_dir, candidate.clone()).unwrap();

    assert!(matches!(outcome, MutationDurability::Confirmed));
    assert_eq!(load_pending(&config_dir).unwrap(), vec![candidate]);
}

#[test]
fn published_pending_write_uncertainty_requires_two_failed_confirmations() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let candidate = PendingRevocation {
        hub_url: "http://127.0.0.1:18080".to_owned(),
        token: "uncertain-pending-token".to_owned(),
    };
    fail_next(&[FaultPoint::WritePublish, FaultPoint::WritePublish]);

    let outcome = enqueue_pending(&config_dir, candidate.clone()).unwrap();

    assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
    assert_eq!(load_pending(&config_dir).unwrap(), vec![candidate]);
}

#[test]
fn persisted_login_requires_a_successful_durability_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    fail_next(&[FaultPoint::WritePublish, FaultPoint::WritePublish]);

    let outcome = store(&config_dir, &login("unconfirmed-login-token")).unwrap();

    assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
    assert_eq!(
        load(&config_dir).unwrap().unwrap().token,
        "unconfirmed-login-token"
    );
}

#[test]
fn changed_unconfirmed_clear_does_not_claim_the_login_was_retained() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    store(&config_dir, &login("removed-token")).unwrap();
    fail_next(&[FaultPoint::Cleanup, FaultPoint::WritePublish]);

    let outcome = clear(&config_dir).unwrap();

    assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
    assert_eq!(load(&config_dir).unwrap(), None);
}

#[test]
fn login_load_and_pending_tombstone_are_one_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let login = login("revoked-token");
    let revocation = PendingRevocation {
        hub_url: login.hub_url.clone(),
        token: login.token.clone(),
    };
    store(&config_dir, &login).unwrap();
    enqueue_pending(&config_dir, revocation.clone()).unwrap();

    let load_config = config_dir.clone();
    let (load_tx, load_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let loader = std::thread::spawn(move || {
        load_tx
            .send(load_after_login_lock(&load_config, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }))
            .unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        LOGIN_FILE_LOCK.try_lock(),
        Err(TryLockError::WouldBlock)
    ));

    let clear_config = config_dir.clone();
    let (clear_tx, clear_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let clearer = std::thread::spawn(move || {
        attempt_tx.send(()).unwrap();
        clear_tx
            .send(clear_matching(&clear_config, &revocation))
            .unwrap();
    });
    attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap();

    release_tx.send(()).unwrap();
    assert_eq!(
        load_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        None
    );
    clear_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    loader.join().unwrap();
    clearer.join().unwrap();
}

#[test]
fn concurrent_enqueues_preserve_every_distinct_revocation() {
    const WORKERS: usize = 24;

    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let barrier = Arc::new(Barrier::new(WORKERS));
    let threads = (0..WORKERS)
        .map(|index| {
            let barrier = Arc::clone(&barrier);
            let config_dir = config_dir.clone();
            std::thread::spawn(move || {
                barrier.wait();
                enqueue_pending(
                    &config_dir,
                    PendingRevocation {
                        hub_url: "http://127.0.0.1:18080".to_owned(),
                        token: format!("concurrent-token-{index:02}"),
                    },
                )
                .unwrap();
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        thread.join().unwrap();
    }

    let mut actual = load_pending(&config_dir)
        .unwrap()
        .into_iter()
        .map(|revocation| revocation.token)
        .collect::<Vec<_>>();
    actual.sort();
    let expected = (0..WORKERS)
        .map(|index| format!("concurrent-token-{index:02}"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn removal_and_enqueue_are_one_serialized_read_modify_write() {
    const PAIRS: usize = 16;

    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    let removals = (0..PAIRS)
        .map(|index| PendingRevocation {
            hub_url: "http://127.0.0.1:18080".to_owned(),
            token: format!("remove-token-{index:02}"),
        })
        .collect::<Vec<_>>();
    for revocation in &removals {
        enqueue_pending(&config_dir, revocation.clone()).unwrap();
    }

    let barrier = Arc::new(Barrier::new(PAIRS * 2));
    let mut threads = Vec::with_capacity(PAIRS * 2);
    for (index, removal) in removals.into_iter().enumerate() {
        let remove_barrier = Arc::clone(&barrier);
        let remove_config = config_dir.clone();
        threads.push(std::thread::spawn(move || {
            remove_barrier.wait();
            remove_pending(&remove_config, &removal).unwrap();
        }));

        let enqueue_barrier = Arc::clone(&barrier);
        let enqueue_config = config_dir.clone();
        threads.push(std::thread::spawn(move || {
            enqueue_barrier.wait();
            enqueue_pending(
                &enqueue_config,
                PendingRevocation {
                    hub_url: "http://127.0.0.1:18080".to_owned(),
                    token: format!("retained-token-{index:02}"),
                },
            )
            .unwrap();
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }

    let mut actual = load_pending(&config_dir)
        .unwrap()
        .into_iter()
        .map(|revocation| revocation.token)
        .collect::<Vec<_>>();
    actual.sort();
    let expected = (0..PAIRS)
        .map(|index| format!("retained-token-{index:02}"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
