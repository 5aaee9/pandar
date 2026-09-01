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
    #[cfg(test)]
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
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_no_auth_retry_arm(
    session_ptr: *mut c_void,
    now_ms: u64,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    session.no_auth_retry_arm(now_ms);
    0
}

#[cfg(test)]
#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_no_auth_retry_active(session_ptr: *mut c_void) -> bool {
    unsafe { session(session_ptr) }.is_some_and(ConnectionSession::no_auth_retry_active)
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_no_auth_retry_begin(
    session_ptr: *mut c_void,
    now_ms: u64,
) -> i32 {
    unsafe { session(session_ptr) }.is_some_and(|session| session.no_auth_retry_begin(now_ms))
        as i32
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_no_auth_retry_complete(
    session_ptr: *mut c_void,
    status: i32,
    now_ms: u64,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    session.no_auth_retry_complete(status, now_ms);
    0
}

#[cfg(test)]
mod tests;
