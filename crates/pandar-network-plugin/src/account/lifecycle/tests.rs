use std::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::{
    account::lifecycle::transaction::{
        PluginAccountBytes, PluginAccountMutation, PluginAccountTransaction, PluginAccountView,
    },
    connection::{
        ffi::{
            pandar_plugin_connection_set_account_epoch,
            pandar_plugin_printer_refresh_session_create,
            pandar_plugin_printer_refresh_session_destroy,
        },
        no_auth_rotation::{NoAuthRotationKey, NoAuthRotationOutcome},
    },
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
        notification: super::transaction::PluginAccountNotification::Silent,
        hub_url: empty,
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
    let session_ptr = pandar_plugin_printer_refresh_session_create(
        hub.as_ptr(),
        hub.len(),
        token.as_ptr(),
        token.len(),
    );
    let session = crate::connection::ffi::session(session_ptr).expect("connection session");
    assert_eq!(
        pandar_plugin_connection_set_account_epoch(session_ptr, 7),
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

struct ProfileAccountState {
    config_dir: String,
    token: String,
    user_id: String,
    user_name: String,
    avatar: String,
    profile_json: String,
    mutation_actions: Vec<i32>,
}

unsafe extern "C" fn profile_account(
    opaque: *mut c_void,
    context: *mut c_void,
    transaction: Option<PluginAccountTransaction>,
) -> i32 {
    let account = unsafe { &mut *opaque.cast::<ProfileAccountState>() };
    let empty = PluginAccountBytes::from_str("");
    let view = PluginAccountView {
        config_dir: PluginAccountBytes::from_str(&account.config_dir),
        hub_url: PluginAccountBytes::from_str("http://hub"),
        token: PluginAccountBytes::from_str(&account.token),
        user_id: PluginAccountBytes::from_str(&account.user_id),
        user_name: PluginAccountBytes::from_str(&account.user_name),
        avatar: PluginAccountBytes::from_str(&account.avatar),
        profile_json: PluginAccountBytes::from_str(&account.profile_json),
        account_epoch: 8,
        config_epoch: 9,
        session_kind: 1,
        transition_pending: 0,
    };
    let mut mutation = PluginAccountMutation {
        action: 0,
        notification: super::transaction::PluginAccountNotification::Silent,
        hub_url: empty,
        token: empty,
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        session_kind: 0,
        error_body: empty,
        http_code: 0,
    };
    let status =
        unsafe { (transaction.expect("account transaction"))(context, &view, &mut mutation) };
    account.mutation_actions.push(mutation.action);
    status
}

fn profile_state(directory: &tempfile::TempDir, token: &str, user_id: &str) -> ProfileAccountState {
    ProfileAccountState {
        config_dir: directory.path().to_str().unwrap().to_owned(),
        token: token.to_owned(),
        user_id: user_id.to_owned(),
        user_name: if user_id.is_empty() {
            String::new()
        } else {
            "Account B".to_owned()
        },
        avatar: if user_id.is_empty() {
            String::new()
        } else {
            "avatar-b".to_owned()
        },
        profile_json: if user_id.is_empty() {
            String::new()
        } else {
            r#"{"user_id":"account-b","user_name":"Account B","tenant_id":"tenant-b","tenant_name":"Tenant B","avatar":"avatar-b"}"#.to_owned()
        },
        mutation_actions: Vec::new(),
    }
}

fn change_user(account: &mut ProfileAccountState, profile: &str) -> NoAuthRotationOutcome {
    let result = unsafe {
        authenticated::pandar_plugin_account_change_user(
            std::ptr::null_mut(),
            0,
            profile.as_ptr(),
            profile.len(),
            (account as *mut ProfileAccountState).cast(),
            Some(profile_account),
        )
    };
    take_http(result.http)
}

fn get_profile(account: &mut ProfileAccountState, token: &str) -> NoAuthRotationOutcome {
    let result = unsafe {
        authenticated::pandar_plugin_account_profile(
            token.as_ptr(),
            token.len(),
            (account as *mut ProfileAccountState).cast(),
            Some(profile_account),
        )
    };
    take_http(result.http)
}

#[test]
fn tokenless_profile_only_confirms_the_current_visible_identity() {
    let stale_directory = tempfile::tempdir().unwrap();
    let mut stale = profile_state(&stale_directory, "account-b-token", "account-b");
    let stale_response = change_user(
        &mut stale,
        r#"{"uidStr":"account-a","account":"Account A","name":"Account A","avatar":"avatar-a"}"#,
    );
    assert_eq!(stale_response.status, 1);
    assert_eq!(stale_response.http_code, 409);
    assert!(stale_response.body.contains("stale_account_response"));
    assert_eq!(stale.mutation_actions, vec![0]);
    assert_eq!(
        std::fs::read_dir(stale_directory.path()).unwrap().count(),
        0
    );

    let matching_directory = tempfile::tempdir().unwrap();
    let mut matching = profile_state(&matching_directory, "account-b-token", "account-b");
    let matching_response = change_user(
        &mut matching,
        r#"{"uidStr":"account-b","account":"Account B","name":"Account B","avatar":"avatar-b"}"#,
    );
    assert_eq!(matching_response.status, 0);
    assert_eq!(matching_response.http_code, 200);
    assert_eq!(matching.mutation_actions, vec![0]);
    assert_eq!(
        std::fs::read_dir(matching_directory.path())
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn profile_checks_a_nonempty_stale_token_before_reporting_unavailability() {
    let active_directory = tempfile::tempdir().unwrap();
    let mut active = profile_state(&active_directory, "account-b-token", "account-b");
    let active_response = get_profile(&mut active, "account-b-token");
    assert_eq!(active_response.status, 0);
    assert_eq!(active_response.http_code, 200);
    assert!(active_response.body.contains("account-b"));
    assert_eq!(active.mutation_actions, vec![0]);

    let stale_directory = tempfile::tempdir().unwrap();
    let mut stale = profile_state(&stale_directory, "", "");
    let stale_response = get_profile(&mut stale, "account-a-token");
    assert_eq!(stale_response.status, 1);
    assert_eq!(stale_response.http_code, 409);
    assert!(stale_response.body.contains("stale_account_response"));
    assert_eq!(stale.mutation_actions, vec![0]);

    let unavailable_directory = tempfile::tempdir().unwrap();
    let mut unavailable = profile_state(&unavailable_directory, "", "");
    let unavailable_response = get_profile(&mut unavailable, "");
    assert_eq!(unavailable_response.status, 1);
    assert_eq!(unavailable_response.http_code, 401);
    assert!(unavailable_response.body.contains("profile_unavailable"));
    assert_eq!(unavailable.mutation_actions, vec![0, 3]);
}

#[test]
fn finished_rotation_follower_binds_only_to_the_original_account_chain() {
    let session = finished_session();
    let mut same_chain = FollowerAccounts {
        calls: AtomicUsize::new(0),
        initial: account("old-a-token", 7),
        after_finished: account("fresh-a-token", 7),
    };
    match recover(
        session,
        expected(),
        (&mut same_chain as *mut FollowerAccounts).cast(),
        Some(follower_account),
    ) {
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
        recover(
            session,
            expected(),
            (&mut switched as *mut FollowerAccounts).cast(),
            Some(follower_account),
        ),
        NoAuthRecovery::Stale
    ));
    pandar_plugin_printer_refresh_session_destroy(session);
}
