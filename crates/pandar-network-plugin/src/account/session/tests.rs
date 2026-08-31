use std::{ffi::c_void, sync::Mutex};

use super::*;
use crate::{
    account::lifecycle::transaction::PluginAccountNotification, connection::ConnectionSession,
    firmware::FirmwarePluginSession,
};

#[derive(Default)]
struct BridgeState {
    replacements: Vec<(String, String, i32)>,
    clears: usize,
    hub_urls: Vec<String>,
    login_statuses: Vec<(i32, bool)>,
    errors: Vec<(u32, String)>,
    preset_resets: usize,
}

static TEST_LOCK: Mutex<()> = Mutex::new(());
static BRIDGE_STATE: Mutex<Option<BridgeState>> = Mutex::new(None);
static REENTRANT_SESSION: Mutex<Option<usize>> = Mutex::new(None);

extern "C" fn replace(
    _: *mut c_void,
    token: PluginAccountBytes,
    _: PluginAccountBytes,
    _: PluginAccountBytes,
    _: PluginAccountBytes,
    _: PluginAccountBytes,
    tenant: PluginAccountBytes,
    kind: i32,
) {
    let mut state = BRIDGE_STATE.lock().unwrap();
    state.as_mut().unwrap().replacements.push((
        unsafe { token.read("token") }.unwrap(),
        unsafe { tenant.read("tenant") }.unwrap(),
        kind,
    ));
}

extern "C" fn clear(_: *mut c_void) {
    BRIDGE_STATE.lock().unwrap().as_mut().unwrap().clears += 1;
}

extern "C" fn set_hub(_: *mut c_void, hub: PluginAccountBytes) {
    BRIDGE_STATE
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .hub_urls
        .push(unsafe { hub.read("hub") }.unwrap());
}

extern "C" fn login(_: *mut c_void, status: i32, logged_in: bool) {
    BRIDGE_STATE
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .login_statuses
        .push((status, logged_in));
    if let Some(address) = REENTRANT_SESSION.lock().unwrap().take() {
        let session = unsafe { &*(address as *const AccountLifecycleSession) };
        session.enqueue(AccountCallback::HttpError(409, "reentrant".into()));
    }
}

extern "C" fn error(_: *mut c_void, code: u32, body: PluginAccountBytes) {
    BRIDGE_STATE
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .errors
        .push((code, unsafe { body.read("error") }.unwrap()));
}

extern "C" fn reset(_: *mut c_void) {
    BRIDGE_STATE.lock().unwrap().as_mut().unwrap().preset_resets += 1;
}

const BRIDGE: PluginAccountSessionBridge = PluginAccountSessionBridge {
    replace,
    clear,
    set_hub_url: set_hub,
    invoke_user_login: login,
    invoke_http_error: error,
    reset_personal_presets: reset,
};

struct Harness {
    _test_lock: std::sync::MutexGuard<'static, ()>,
    account: AccountLifecycleSession,
    connection: Box<ConnectionSession>,
    firmware: Box<FirmwarePluginSession>,
}

impl Harness {
    fn new() -> Self {
        let test_lock = TEST_LOCK.lock().unwrap();
        *BRIDGE_STATE.lock().unwrap() = Some(BridgeState::default());
        let mut connection = Box::new(ConnectionSession::new("https://hub-a".into(), "old".into()));
        assert_eq!(
            unsafe {
                crate::connection::pandar_plugin_connection_set_account_epoch(
                    (&mut *connection as *mut ConnectionSession).cast(),
                    7,
                )
            },
            0
        );
        Self {
            _test_lock: test_lock,
            account: AccountLifecycleSession::new(),
            connection,
            firmware: Box::new(FirmwarePluginSession::new(
                "https://hub-a".into(),
                "old".into(),
                1,
            )),
        }
    }

    fn apply(&mut self, action: i32, notification: PluginAccountNotification) {
        let profile =
            r#"{"user_id":"user","user_name":"User","tenant_id":"tenant","tenant_name":"Tenant"}"#;
        let token = "new";
        let hub = "https://hub-b";
        let error = "failure";
        let mutation = PluginAccountMutation {
            action,
            notification,
            hub_url: PluginAccountBytes::from_str(hub),
            token: PluginAccountBytes::from_str(token),
            user_id: PluginAccountBytes::from_str("user"),
            user_name: PluginAccountBytes::from_str("User"),
            avatar: PluginAccountBytes::from_str(""),
            profile_json: PluginAccountBytes::from_str(profile),
            session_kind: 1,
            error_body: PluginAccountBytes::from_str(error),
            http_code: 503,
        };
        let current = crate::account::lifecycle::transaction::PluginAccountView {
            config_dir: PluginAccountBytes::from_str("/tmp"),
            hub_url: PluginAccountBytes::from_str("https://hub-a"),
            token: PluginAccountBytes::from_str("old"),
            user_id: PluginAccountBytes::from_str("old-user"),
            user_name: PluginAccountBytes::from_str("Old"),
            avatar: PluginAccountBytes::from_str(""),
            profile_json: PluginAccountBytes::from_str(profile),
            account_epoch: 7,
            config_epoch: 11,
            session_kind: 1,
            transition_pending: 0,
        };
        let status = unsafe {
            pandar_plugin_account_session_apply(
                (&mut self.account as *mut AccountLifecycleSession).cast(),
                (&mut *self.connection as *mut ConnectionSession).cast(),
                (&mut *self.firmware as *mut FirmwarePluginSession).cast(),
                &BRIDGE,
                std::ptr::null_mut(),
                &current,
                &mutation,
            )
        };
        assert_eq!(status, 0);
    }

    fn callbacks(&self) -> Vec<&'static str> {
        self.account
            .callbacks
            .lock()
            .unwrap()
            .pending
            .iter()
            .map(|callback| match callback {
                AccountCallback::Transition(_) => "transition",
                AccountCallback::UserLogin(_) => "login",
                AccountCallback::HttpError(_, _) => "http_error",
            })
            .collect()
    }
}

#[test]
fn replace_mutation_applies_account_without_transition() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_REPLACE, PluginAccountNotification::Silent);

    let state = BRIDGE_STATE.lock().unwrap();
    let state = state.as_ref().unwrap();
    assert_eq!(
        state.replacements,
        [(String::from("new"), String::from("tenant"), 1)]
    );
    assert_eq!(state.preset_resets, 1);
    assert!(harness.callbacks().is_empty());
    assert_eq!(harness.firmware.generation(), 2);
}

#[test]
fn login_mutation_orders_transition_before_login_notification() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_LOGIN, PluginAccountNotification::Silent);

    assert_eq!(harness.callbacks(), ["transition"]);
    let callbacks = harness.account.callbacks.lock().unwrap();
    let AccountCallback::Transition(callback) = callbacks.pending.front().unwrap() else {
        unreachable!();
    };
    assert_eq!(callback.account_epoch, 8);
    assert_eq!(callback.notification, Some(true));
    assert_eq!(harness.firmware.generation(), 3);
}

#[test]
fn clear_mutation_queues_guarded_logout_after_transition() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_CLEAR, PluginAccountNotification::Logout);

    assert_eq!(BRIDGE_STATE.lock().unwrap().as_ref().unwrap().clears, 1);
    let callbacks = harness.account.callbacks.lock().unwrap();
    let AccountCallback::Transition(callback) = callbacks.pending.front().unwrap() else {
        unreachable!();
    };
    assert_eq!(callback.notification, Some(false));
    assert_eq!(callback.expected.as_ref().unwrap().token, "");
}

#[test]
fn http_error_mutation_only_queues_the_error() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_HTTP_ERROR, PluginAccountNotification::Silent);

    assert_eq!(harness.callbacks(), ["http_error"]);
    assert_eq!(
        BRIDGE_STATE.lock().unwrap().as_ref().unwrap().preset_resets,
        0
    );
}

#[test]
fn runtime_hub_mutation_clears_account_before_transition_delivery() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_RUNTIME_HUB, PluginAccountNotification::Silent);

    let state = BRIDGE_STATE.lock().unwrap();
    let state = state.as_ref().unwrap();
    assert_eq!(state.clears, 1);
    assert_eq!(state.hub_urls, ["https://hub-b"]);
    assert_eq!(harness.callbacks(), ["transition"]);
}

#[test]
fn firmware_fence_mutation_advances_only_the_rust_generation() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_FIRMWARE_FENCE, PluginAccountNotification::Silent);

    assert_eq!(harness.firmware.generation(), 2);
    assert!(harness.callbacks().is_empty());
    assert_eq!(
        BRIDGE_STATE.lock().unwrap().as_ref().unwrap().preset_resets,
        0
    );
}

#[test]
fn restore_failure_mutation_orders_transition_login_and_guarded_error() {
    let mut harness = Harness::new();
    harness.apply(MUTATION_RESTORE_FAILURE, PluginAccountNotification::Silent);

    let callbacks = harness.account.callbacks.lock().unwrap();
    let AccountCallback::Transition(callback) = callbacks.pending.front().unwrap() else {
        unreachable!();
    };
    assert_eq!(callback.notification, Some(true));
    assert_eq!(callback.error, Some((503, String::from("failure"))));
    assert_eq!(callback.expected.as_ref().unwrap().token, "new");
}

#[test]
fn lifecycle_notifications_preserve_login_status_payload_policy() {
    let _test_lock = TEST_LOCK.lock().unwrap();
    *BRIDGE_STATE.lock().unwrap() = Some(BridgeState::default());
    let mut session = Box::new(AccountLifecycleSession::new());
    let mut connection = Box::new(ConnectionSession::new("http://hub".into(), String::new()));
    session.enqueue(AccountCallback::UserLogin(true));
    session.enqueue(AccountCallback::UserLogin(false));
    unsafe {
        super::callbacks::pandar_plugin_account_session_drain(
            (&mut *session as *mut AccountLifecycleSession).cast(),
            (&mut *connection as *mut ConnectionSession).cast(),
            std::ptr::null(),
            &BRIDGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            None,
        );
    }

    assert_eq!(
        BRIDGE_STATE
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .login_statuses,
        [(1, true), (0, false)]
    );
}

#[test]
fn reentrant_login_callback_is_drained_after_the_current_callback() {
    let _test_lock = TEST_LOCK.lock().unwrap();
    *BRIDGE_STATE.lock().unwrap() = Some(BridgeState::default());
    let mut session = Box::new(AccountLifecycleSession::new());
    let mut connection = Box::new(ConnectionSession::new("http://hub".into(), String::new()));
    *REENTRANT_SESSION.lock().unwrap() =
        Some((&*session as *const AccountLifecycleSession) as usize);
    session.enqueue(AccountCallback::UserLogin(true));

    unsafe {
        super::callbacks::pandar_plugin_account_session_drain(
            (&mut *session as *mut AccountLifecycleSession).cast(),
            (&mut *connection as *mut ConnectionSession).cast(),
            std::ptr::null(),
            &BRIDGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            None,
        );
    }

    let state = BRIDGE_STATE.lock().unwrap();
    let state = state.as_ref().unwrap();
    assert_eq!(state.login_statuses, [(1, true)]);
    assert_eq!(state.errors, [(409, String::from("reentrant"))]);
    assert!(session.callbacks.lock().unwrap().pending.is_empty());
}

#[test]
fn logout_guard_rejects_a_relogged_account() {
    let expected = ExpectedAccount {
        hub_url: "http://hub".into(),
        token: String::new(),
        account_epoch: 8,
        config_epoch: 3,
        session_kind: 0,
    };
    let relogged = AccountView {
        config_dir: "/tmp".into(),
        hub_url: "http://hub".into(),
        token: "new-login".into(),
        user_id: "user".into(),
        user_name: "User".into(),
        avatar: String::new(),
        profile_json: String::new(),
        account_epoch: 9,
        config_epoch: 3,
        session_kind: 1,
        transition_pending: false,
    };

    assert!(!expected.matches_view(&relogged));
}
