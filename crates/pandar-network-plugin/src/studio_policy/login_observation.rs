use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::read_utf8;

use super::{ACCOUNT_ACTION_FAILURE, ACCOUNT_ACTION_LOGOUT, ACCOUNT_ACTION_NONE};

static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct LoginObservation {
    identity: u64,
    account_epoch: u64,
    token: String,
}

thread_local! {
    static LOGIN_OBSERVATION: RefCell<Option<LoginObservation>> = const { RefCell::new(None) };
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_identity_create() -> u64 {
    NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_observe_login(
    identity: u64,
    account_epoch: u64,
    token_ptr: *const u8,
    token_len: usize,
) -> bool {
    let Some(token) = read_utf8(token_ptr, token_len) else {
        clear(identity);
        return false;
    };
    LOGIN_OBSERVATION.with(|observation| {
        *observation.borrow_mut() = Some(LoginObservation {
            identity,
            account_epoch,
            token: token.to_owned(),
        });
    });
    !token.is_empty()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_login_observation_clear(identity: u64) {
    clear(identity);
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_logout_action(
    identity: u64,
    request: bool,
    current_epoch: u64,
    token_ptr: *const u8,
    token_len: usize,
) -> i32 {
    let Some(current_token) = read_utf8(token_ptr, token_len) else {
        clear(identity);
        return ACCOUNT_ACTION_FAILURE;
    };
    let observation = take(identity);
    if request {
        return ACCOUNT_ACTION_LOGOUT;
    }
    if current_token.is_empty() {
        return ACCOUNT_ACTION_NONE;
    }
    match observation {
        Some(observation)
            if observation.account_epoch != current_epoch || observation.token != current_token =>
        {
            ACCOUNT_ACTION_NONE
        }
        _ => ACCOUNT_ACTION_LOGOUT,
    }
}

fn take(identity: u64) -> Option<LoginObservation> {
    LOGIN_OBSERVATION.with(|observation| {
        let mut observation = observation.borrow_mut();
        if observation
            .as_ref()
            .is_some_and(|observation| observation.identity == identity)
        {
            observation.take()
        } else {
            None
        }
    })
}

fn clear(identity: u64) {
    let _ = take(identity);
}
