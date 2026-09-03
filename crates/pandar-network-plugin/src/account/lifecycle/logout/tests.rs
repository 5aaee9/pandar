use super::*;
use crate::{
    account::lifecycle::transaction::{
        PluginAccountMutation, PluginAccountNotification, PluginAccountTransaction,
        PluginAccountView,
    },
    account::types::PendingRevocation,
    connection::ffi::{
        pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    },
    studio_policy::login_observation::pandar_plugin_account_identity_create,
};

#[test]
fn empty_config_directory_cannot_claim_a_durable_revocation() {
    let candidate = PendingRevocation {
        hub_url: "http://127.0.0.1:18080".to_owned(),
        token: "token".to_owned(),
    };
    assert!(matches!(
        stage_revocation("", &candidate),
        RevocationStage::Failed
    ));
}

struct EmptyAccount {
    session: *mut c_void,
    identity: u64,
    config_dir: String,
    account_epoch: u64,
    reentered: bool,
    calls: usize,
    actions: Vec<i32>,
}

unsafe extern "C" fn empty_account(
    opaque: *mut c_void,
    context: *mut c_void,
    transaction: Option<PluginAccountTransaction>,
) -> i32 {
    let account = unsafe { &mut *opaque.cast::<EmptyAccount>() };
    account.calls += 1;
    let empty = PluginAccountBytes::from_str("");
    let view = PluginAccountView {
        config_dir: PluginAccountBytes::from_str(&account.config_dir),
        hub_url: PluginAccountBytes::from_str("http://127.0.0.1:8080"),
        frontend_url: empty,
        token: empty,
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        account_epoch: account.account_epoch,
        config_epoch: 0,
        session_kind: 0,
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
    let status = unsafe { transaction.unwrap()(context, &view, &mut mutation) };
    account.actions.push(mutation.action);
    if mutation.action == MUTATION_CLEAR {
        account.account_epoch = account.account_epoch.wrapping_add(1);
    }
    if !account.reentered {
        account.reentered = true;
        let result = unsafe {
            pandar_plugin_account_logout(
                account.session,
                account.identity,
                true,
                opaque,
                Some(empty_account),
            )
        };
        assert_eq!(take_http(result.http).status, 0);
    }
    status
}

#[test]
fn requested_upgrade_replays_a_passive_empty_transaction_to_fence_the_epoch() {
    let directory = tempfile::tempdir().unwrap();
    let hub = b"http://127.0.0.1:8080";
    let token = b"";
    let session = unsafe {
        pandar_plugin_printer_refresh_session_create(
            hub.as_ptr(),
            hub.len(),
            token.as_ptr(),
            token.len(),
        )
    };
    let mut account = EmptyAccount {
        session,
        identity: pandar_plugin_account_identity_create(),
        config_dir: directory.path().to_str().unwrap().to_owned(),
        account_epoch: 7,
        reentered: false,
        calls: 0,
        actions: Vec::new(),
    };
    let result = unsafe {
        pandar_plugin_account_logout(
            session,
            account.identity,
            false,
            (&mut account as *mut EmptyAccount).cast(),
            Some(empty_account),
        )
    };
    let outcome = take_http(result.http);
    assert_eq!(account.calls, 2);
    assert_eq!(account.actions, vec![ACCOUNT_ACTION_NONE, MUTATION_CLEAR]);
    assert_eq!(account.account_epoch, 8);
    assert_eq!(outcome.status, 0, "{outcome:?}");
    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}
