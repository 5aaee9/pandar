use std::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::super::{NoAuthExpected, NoAuthRecovery, recover};
use crate::account::lifecycle::transaction::{
    PluginAccountBytes, PluginAccountMutation, PluginAccountNotification, PluginAccountTransaction,
    PluginAccountView,
};
use crate::connection::{
    ffi::{
        pandar_plugin_connection_set_account_epoch, pandar_plugin_printer_refresh_session_create,
        pandar_plugin_printer_refresh_session_destroy,
    },
    no_auth_rotation::{NoAuthRotationBegin, NoAuthRotationKey, NoAuthRotationOutcome},
};

struct FollowerAccounts {
    calls: AtomicUsize,
    initial: AccountState,
    after_finished: AccountState,
}

struct AccountState {
    hub_url: String,
    token: String,
    account_epoch: u64,
    config_epoch: u64,
    session_kind: i32,
}

unsafe extern "C" fn follower_account(
    opaque: *mut c_void,
    context: *mut c_void,
    transaction: Option<PluginAccountTransaction>,
) -> i32 {
    let accounts = unsafe { &*opaque.cast::<FollowerAccounts>() };
    let state = if accounts.calls.fetch_add(1, Ordering::SeqCst) == 0 {
        &accounts.initial
    } else {
        &accounts.after_finished
    };
    let empty = PluginAccountBytes::from_str("");
    let view = PluginAccountView {
        config_dir: empty,
        hub_url: PluginAccountBytes::from_str(&state.hub_url),
        frontend_url: empty,
        token: PluginAccountBytes::from_str(&state.token),
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        account_epoch: state.account_epoch,
        config_epoch: state.config_epoch,
        session_kind: state.session_kind,
        transition_pending: 0,
    };
    let mut mutation = PluginAccountMutation {
        action: 0,
        notification: PluginAccountNotification::Silent,
        hub_url: empty,
        frontend_url: empty,
        token: empty,
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        session_kind: 0,
        error_body: empty,
        http_code: 0,
    };
    unsafe { (transaction.expect("account transaction"))(context, &view, &mut mutation) }
}

fn account(token: &str, account_epoch: u64) -> AccountState {
    AccountState {
        hub_url: "http://127.0.0.1:1".to_owned(),
        token: token.to_owned(),
        account_epoch,
        config_epoch: 9,
        session_kind: 2,
    }
}

fn expected() -> NoAuthExpected {
    NoAuthExpected {
        hub_url: "http://127.0.0.1:1".to_owned(),
        token: "old-a-token".to_owned(),
        account_epoch: 7,
        config_epoch: 9,
        session_kind: 2,
    }
}

fn finished_session() -> *mut c_void {
    let hub = b"http://127.0.0.1:1";
    let token = b"old-a-token";
    let session_ptr = unsafe {
        pandar_plugin_printer_refresh_session_create(
            hub.as_ptr(),
            hub.len(),
            token.as_ptr(),
            token.len(),
        )
    };
    let session =
        unsafe { crate::connection::ffi::session(session_ptr) }.expect("connection session");
    assert_eq!(
        unsafe { pandar_plugin_connection_set_account_epoch(session_ptr, 7) },
        0
    );
    let key = NoAuthRotationKey::new(
        "http://127.0.0.1:1".to_owned(),
        "old-a-token".to_owned(),
        7,
        9,
    );
    assert_eq!(
        session.begin_no_auth_rotation(key.clone()),
        NoAuthRotationBegin::Started
    );
    assert!(session.finish_no_auth_rotation(
        key,
        NoAuthRotationOutcome {
            status: 0,
            http_code: 200,
            body: String::new(),
        },
    ));
    session_ptr
}

#[test]
fn finished_rotation_follower_binds_only_to_the_original_account_chain() {
    let session = finished_session();
    let mut same_chain = FollowerAccounts {
        calls: AtomicUsize::new(0),
        initial: account("old-a-token", 7),
        after_finished: account("fresh-a-token", 7),
    };
    match unsafe {
        recover(
            session,
            expected(),
            (&mut same_chain as *mut FollowerAccounts).cast(),
            Some(follower_account),
        )
    } {
        NoAuthRecovery::Recovered(identity) => {
            assert_eq!(identity.token, "fresh-a-token");
            assert_eq!(identity.account_epoch, 7);
            assert_eq!(identity.config_epoch, 9);
        }
        other => panic!("same-chain follower did not recover: {other:?}"),
    }

    let mut switched = FollowerAccounts {
        calls: AtomicUsize::new(0),
        initial: account("old-a-token", 7),
        after_finished: account("account-b-token", 8),
    };
    assert!(matches!(
        unsafe {
            recover(
                session,
                expected(),
                (&mut switched as *mut FollowerAccounts).cast(),
                Some(follower_account),
            )
        },
        NoAuthRecovery::Stale
    ));
    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}
