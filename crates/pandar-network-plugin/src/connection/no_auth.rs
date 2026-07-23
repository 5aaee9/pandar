use std::ffi::c_void;

use super::{ConnectionSession, ffi::session};

const INITIAL_RETRY_DELAY_MS: u64 = 2_000;
const MAX_RETRY_DELAY_MS: u64 = 30_000;
const MAX_AUTO_ATTEMPTS: u8 = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RetryKey {
    generation: u64,
    account_epoch: u64,
}

enum RetryPhase {
    Unarmed,
    Waiting {
        key: RetryKey,
        next_attempt_ms: u64,
        delay_ms: u64,
        attempts_started: u8,
    },
    InFlight {
        key: RetryKey,
        delay_ms: u64,
        attempts_started: u8,
    },
    Finished {
        key: RetryKey,
    },
}

pub(super) struct NoAuthRetry {
    phase: RetryPhase,
}

impl Default for NoAuthRetry {
    fn default() -> Self {
        Self {
            phase: RetryPhase::Unarmed,
        }
    }
}

impl ConnectionSession {
    fn no_auth_retry_active(&self) -> bool {
        let state = self.state.lock().expect("connection state");
        if !state.token.trim().is_empty() {
            return false;
        }
        let current = RetryKey {
            generation: state.generation,
            account_epoch: state.account_epoch,
        };
        match &state.no_auth_retry.phase {
            RetryPhase::Waiting { key, .. } | RetryPhase::InFlight { key, .. } => *key == current,
            RetryPhase::Unarmed | RetryPhase::Finished { .. } => false,
        }
    }

    fn no_auth_retry_arm(&self, now_ms: u64) {
        let mut state = self.state.lock().expect("connection state");
        let key = RetryKey {
            generation: state.generation,
            account_epoch: state.account_epoch,
        };
        if !state.token.trim().is_empty() {
            state.no_auth_retry.phase = RetryPhase::Finished { key };
            return;
        }
        let same_key = match state.no_auth_retry.phase {
            RetryPhase::Unarmed => false,
            RetryPhase::Waiting { key: current, .. }
            | RetryPhase::InFlight { key: current, .. }
            | RetryPhase::Finished { key: current } => current == key,
        };
        if !same_key {
            state.no_auth_retry.phase = RetryPhase::Waiting {
                key,
                next_attempt_ms: now_ms,
                delay_ms: INITIAL_RETRY_DELAY_MS,
                attempts_started: 0,
            };
        }
    }

    fn no_auth_retry_begin(&self, now_ms: u64) -> bool {
        let mut state = self.state.lock().expect("connection state");
        let key = RetryKey {
            generation: state.generation,
            account_epoch: state.account_epoch,
        };
        if !state.token.trim().is_empty() {
            state.no_auth_retry.phase = RetryPhase::Finished { key };
            return false;
        }
        let RetryPhase::Waiting {
            key: expected,
            next_attempt_ms,
            delay_ms,
            attempts_started,
        } = state.no_auth_retry.phase
        else {
            return false;
        };
        if expected != key {
            state.no_auth_retry.phase = RetryPhase::Unarmed;
            return false;
        }
        if now_ms < next_attempt_ms {
            return false;
        }
        if attempts_started >= MAX_AUTO_ATTEMPTS {
            state.no_auth_retry.phase = RetryPhase::Finished { key };
            return false;
        }
        state.no_auth_retry.phase = RetryPhase::InFlight {
            key,
            delay_ms,
            attempts_started: attempts_started + 1,
        };
        true
    }

    fn no_auth_retry_complete(&self, status: i32, now_ms: u64) {
        let mut state = self.state.lock().expect("connection state");
        let current_key = RetryKey {
            generation: state.generation,
            account_epoch: state.account_epoch,
        };
        let phase = std::mem::replace(&mut state.no_auth_retry.phase, RetryPhase::Unarmed);
        let RetryPhase::InFlight {
            key,
            delay_ms,
            attempts_started,
        } = phase
        else {
            state.no_auth_retry.phase = phase;
            return;
        };
        if key != current_key || !state.token.trim().is_empty() {
            state.no_auth_retry.phase = RetryPhase::Finished { key: current_key };
        } else if crate::pandar_plugin_no_auth_retryable_connect_failure(status)
            && attempts_started < MAX_AUTO_ATTEMPTS
        {
            state.no_auth_retry.phase = RetryPhase::Waiting {
                key,
                next_attempt_ms: now_ms.saturating_add(delay_ms),
                delay_ms: delay_ms.saturating_mul(2).min(MAX_RETRY_DELAY_MS),
                attempts_started,
            };
        } else {
            state.no_auth_retry.phase = RetryPhase::Finished { key };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_no_auth_retry_arm(session_ptr: *mut c_void, now_ms: u64) -> i32 {
    let Some(session) = session(session_ptr) else {
        return 1;
    };
    session.no_auth_retry_arm(now_ms);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_no_auth_retry_active(session_ptr: *mut c_void) -> bool {
    session(session_ptr).is_some_and(ConnectionSession::no_auth_retry_active)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_no_auth_retry_begin(session_ptr: *mut c_void, now_ms: u64) -> i32 {
    session(session_ptr).is_some_and(|session| session.no_auth_retry_begin(now_ms)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_no_auth_retry_complete(
    session_ptr: *mut c_void,
    status: i32,
    now_ms: u64,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return 1;
    };
    session.no_auth_retry_complete(status, now_ms);
    0
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    use crate::{
        pandar_plugin_connection_set_account_epoch,
        pandar_plugin_no_auth_retryable_connect_failure,
        pandar_plugin_printer_refresh_session_create,
        pandar_plugin_printer_refresh_session_destroy,
        pandar_plugin_printer_refresh_session_update,
    };

    use super::{
        pandar_plugin_no_auth_retry_active, pandar_plugin_no_auth_retry_arm,
        pandar_plugin_no_auth_retry_begin, pandar_plugin_no_auth_retry_complete,
    };

    fn session() -> *mut std::ffi::c_void {
        let hub_url = "http://127.0.0.1:8080";
        let token = "";
        pandar_plugin_printer_refresh_session_create(
            hub_url.as_ptr(),
            hub_url.len(),
            token.as_ptr(),
            token.len(),
        )
    }

    #[test]
    fn no_auth_retry_active_only_while_waiting_or_in_flight() {
        let session = session();
        assert!(!session.is_null());
        assert!(!pandar_plugin_no_auth_retry_active(session));

        assert_eq!(pandar_plugin_no_auth_retry_arm(session, 1_000), 0);
        assert!(pandar_plugin_no_auth_retry_active(session));
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, 1_000), 1);
        assert!(pandar_plugin_no_auth_retry_active(session));
        assert_eq!(pandar_plugin_no_auth_retry_complete(session, 0, 1_000), 0);
        assert!(!pandar_plugin_no_auth_retry_active(session));

        pandar_plugin_printer_refresh_session_destroy(session);
    }

    #[test]
    fn connect_failure_leaves_no_auth_retry_active_while_waiting() {
        let session = session();
        assert_eq!(pandar_plugin_no_auth_retry_arm(session, 1_000), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, 1_000), 1);
        assert_eq!(pandar_plugin_no_auth_retry_complete(session, 2, 1_000), 0);
        assert!(pandar_plugin_no_auth_retry_active(session));

        pandar_plugin_printer_refresh_session_destroy(session);
    }

    #[test]
    fn no_auth_retry_claims_one_attempt_and_only_rearms_after_connect_failure_delay() {
        let session = session();
        assert!(!session.is_null());
        assert_eq!(pandar_plugin_no_auth_retry_arm(session, 1_000), 0);

        let barrier = Arc::new(Barrier::new(33));
        let attempts = (0..32)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let session = session as usize;
                thread::spawn(move || {
                    barrier.wait();
                    pandar_plugin_no_auth_retry_begin(session as *mut _, 1_000)
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
            pandar_plugin_no_auth_retry_complete(session, connect_failure_status, 1_000),
            0
        );
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, 2_999), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, 3_000), 1);

        assert_eq!(pandar_plugin_no_auth_retry_complete(session, 1, 3_000), 0);
        assert_eq!(pandar_plugin_no_auth_retry_arm(session, 3_000), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, u64::MAX), 0);

        pandar_plugin_printer_refresh_session_destroy(session);
    }

    #[test]
    fn no_auth_retry_stops_after_five_connect_failures() {
        let session = session();
        assert!(!session.is_null());
        assert_eq!(pandar_plugin_no_auth_retry_arm(session, 0), 0);

        for _ in 0..5 {
            assert_eq!(pandar_plugin_no_auth_retry_begin(session, u64::MAX), 1);
            assert_eq!(
                pandar_plugin_no_auth_retry_complete(session, 2, u64::MAX),
                0
            );
        }

        assert_eq!(pandar_plugin_no_auth_retry_begin(session, u64::MAX), 0);
        assert_eq!(pandar_plugin_no_auth_retry_arm(session, u64::MAX), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(session, u64::MAX), 0);

        pandar_plugin_printer_refresh_session_destroy(session);
    }

    #[test]
    fn no_auth_retry_is_fenced_by_token_account_and_hub_changes() {
        let connect_failure_status = 2;

        let token_session = session();
        assert_eq!(pandar_plugin_no_auth_retry_arm(token_session, 0), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(token_session, 0), 1);
        assert_eq!(
            pandar_plugin_no_auth_retry_complete(token_session, connect_failure_status, 0),
            0
        );
        let hub_url = "http://127.0.0.1:8080";
        let token = "new-token";
        assert_eq!(
            pandar_plugin_printer_refresh_session_update(
                token_session,
                hub_url.as_ptr(),
                hub_url.len(),
                token.as_ptr(),
                token.len(),
            ),
            0
        );
        assert_eq!(
            pandar_plugin_no_auth_retry_begin(token_session, u64::MAX),
            0
        );
        pandar_plugin_printer_refresh_session_destroy(token_session);

        let account_session = session();
        assert_eq!(pandar_plugin_no_auth_retry_arm(account_session, 0), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(account_session, 0), 1);
        assert_eq!(
            pandar_plugin_no_auth_retry_complete(account_session, connect_failure_status, 0),
            0
        );
        assert_eq!(
            pandar_plugin_connection_set_account_epoch(account_session, 1),
            0
        );
        assert!(!pandar_plugin_no_auth_retry_active(account_session));
        assert_eq!(
            pandar_plugin_no_auth_retry_begin(account_session, u64::MAX),
            0
        );
        pandar_plugin_printer_refresh_session_destroy(account_session);

        let hub_session = session();
        assert_eq!(pandar_plugin_no_auth_retry_arm(hub_session, 0), 0);
        assert_eq!(pandar_plugin_no_auth_retry_begin(hub_session, 0), 1);
        assert_eq!(
            pandar_plugin_no_auth_retry_complete(hub_session, connect_failure_status, 0),
            0
        );
        let new_hub_url = "http://127.0.0.1:8081";
        let empty_token = "";
        assert_eq!(
            pandar_plugin_printer_refresh_session_update(
                hub_session,
                new_hub_url.as_ptr(),
                new_hub_url.len(),
                empty_token.as_ptr(),
                empty_token.len(),
            ),
            0
        );
        assert_eq!(pandar_plugin_no_auth_retry_begin(hub_session, u64::MAX), 0);
        pandar_plugin_printer_refresh_session_destroy(hub_session);
    }
}
