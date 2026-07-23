use std::{
    ffi::c_void,
    io::{Read, Write},
    net::TcpListener,
    slice,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const MUTATION_RESTORE_FAILURE: i32 = 8;

use super::*;
use crate::{
    account::lifecycle::transaction::{
        PluginAccountMutation, PluginAccountNotification, PluginAccountTransaction,
        PluginAccountView,
    },
    connection::ffi::{
        pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    },
    studio_policy::login_observation::pandar_plugin_account_identity_create,
};

struct RaceState {
    hub_url: String,
    token: String,
    account_epoch: u64,
    session_kind: i32,
}

struct RaceAccount {
    config_dir: String,
    state: Mutex<RaceState>,
    actions: Mutex<Vec<(i32, i32)>>,
    block_clear: AtomicBool,
    clear_entered: (Mutex<bool>, Condvar),
    release_clear: (Mutex<bool>, Condvar),
    block_return: AtomicBool,
    return_entered: (Mutex<bool>, Condvar),
    release_return: (Mutex<bool>, Condvar),
}

unsafe extern "C" fn race_account(
    opaque: *mut c_void,
    context: *mut c_void,
    transaction: Option<PluginAccountTransaction>,
) -> i32 {
    let account = unsafe { &*opaque.cast::<RaceAccount>() };
    let (hub_url, token, account_epoch, session_kind) = {
        let state = account.state.lock().unwrap();
        (
            state.hub_url.clone(),
            state.token.clone(),
            state.account_epoch,
            state.session_kind,
        )
    };
    let empty = PluginAccountBytes::from_str("");
    let view = PluginAccountView {
        config_dir: PluginAccountBytes::from_str(&account.config_dir),
        hub_url: PluginAccountBytes::from_str(&hub_url),
        token: PluginAccountBytes::from_str(&token),
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        account_epoch,
        config_epoch: 0,
        session_kind,
        transition_pending: 0,
    };
    let mut mutation = PluginAccountMutation {
        action: 0,
        notification: PluginAccountNotification::Silent,
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
    let status = unsafe { transaction.unwrap()(context, &view, &mut mutation) };
    account
        .actions
        .lock()
        .unwrap()
        .push((mutation.action, mutation.notification as i32));
    if mutation.action == MUTATION_CLEAR {
        let mut state = account.state.lock().unwrap();
        state.token.clear();
        state.account_epoch = state.account_epoch.wrapping_add(1);
        state.session_kind = 0;
        drop(state);
        if account.block_clear.swap(false, Ordering::AcqRel) {
            let mut entered = account.clear_entered.0.lock().unwrap();
            *entered = true;
            account.clear_entered.1.notify_all();
            drop(entered);
            let release = account.release_clear.0.lock().unwrap();
            drop(
                account
                    .release_clear
                    .1
                    .wait_while(release, |release| !*release)
                    .unwrap(),
            );
        }
    } else if mutation.action == MUTATION_RESTORE_FAILURE {
        let mut state = account.state.lock().unwrap();
        state.token = unsafe {
            String::from_utf8(
                slice::from_raw_parts(mutation.token.ptr, mutation.token.len).to_vec(),
            )
            .unwrap()
        };
        state.account_epoch = state.account_epoch.wrapping_add(1);
        state.session_kind = mutation.session_kind;
    }
    if account.block_return.swap(false, Ordering::AcqRel) {
        let mut entered = account.return_entered.0.lock().unwrap();
        *entered = true;
        account.return_entered.1.notify_all();
        drop(entered);
        let release = account.release_return.0.lock().unwrap();
        drop(
            account
                .release_return
                .1
                .wait_while(release, |release| !*release)
                .unwrap(),
        );
    }
    status
}

fn run_logout(
    session: usize,
    identity: u64,
    request: bool,
    account: usize,
) -> NoAuthRotationOutcome {
    let result = unsafe {
        pandar_plugin_account_logout(
            session as *mut c_void,
            identity,
            request,
            account as *mut c_void,
            Some(race_account),
        )
    };
    take_http(result.http)
}

#[test]
fn external_requested_upgrades_passive_finalizing_and_gets_exact_delete_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
        }
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.starts_with("delete /api/v1/plugin/session http/1.1"));
        assert!(request.contains("authorization: bearer external-upgrade-secret-token"));
        let body =
            r#"{"error":"raw-external-delete-failure","token":"external-upgrade-secret-token"}"#;
        write!(
            stream,
            "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });

    let directory = tempfile::tempdir().unwrap();
    let token = "external-upgrade-secret-token";
    let session = pandar_plugin_printer_refresh_session_create(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
    );
    let identity = pandar_plugin_account_identity_create();
    let account = Arc::new(RaceAccount {
        config_dir: directory.path().to_str().unwrap().to_owned(),
        state: Mutex::new(RaceState {
            hub_url,
            token: token.to_owned(),
            account_epoch: 11,
            session_kind: 1,
        }),
        actions: Mutex::new(Vec::new()),
        block_clear: AtomicBool::new(true),
        clear_entered: (Mutex::new(false), Condvar::new()),
        release_clear: (Mutex::new(false), Condvar::new()),
        block_return: AtomicBool::new(false),
        return_entered: (Mutex::new(false), Condvar::new()),
        release_return: (Mutex::new(false), Condvar::new()),
    });
    let session_address = session as usize;
    let account_address = Arc::as_ptr(&account) as usize;
    let passive =
        thread::spawn(move || run_logout(session_address, identity, false, account_address));
    let entered = account.clear_entered.0.lock().unwrap();
    drop(
        account
            .clear_entered
            .1
            .wait_while(entered, |entered| !*entered)
            .unwrap(),
    );

    let requested =
        thread::spawn(move || run_logout(session_address, identity, true, account_address));
    crate::connection::ffi::session(session)
        .unwrap()
        .wait_for_account_logout_follower();
    *account.release_clear.0.lock().unwrap() = true;
    account.release_clear.1.notify_all();

    let passive = passive.join().unwrap();
    let requested = requested.join().unwrap();
    server.join().unwrap();
    let expected_body = r#"{"error":"invalid_response"}"#;
    for outcome in [passive, requested] {
        assert_eq!(outcome.status, 1);
        assert_eq!(outcome.http_code, 503);
        assert_eq!(outcome.body, expected_body);
        assert!(!outcome.body.contains("external-upgrade-secret-token"));
    }
    assert_eq!(
        *account.actions.lock().unwrap(),
        vec![
            (MUTATION_CLEAR, PluginAccountNotification::Logout as i32),
            (
                MUTATION_HTTP_ERROR,
                PluginAccountNotification::Silent as i32
            ),
        ]
    );
    let state = account.state.lock().unwrap();
    assert!(state.token.is_empty());
    assert_eq!(state.account_epoch, 12);
    assert!(
        directory
            .path()
            .join("pandar-plugin-pending-revocations.json")
            .exists()
    );
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn external_requested_upgrade_keeps_direct_intent_when_delete_is_ambiguous() {
    use crate::account::{
        persistence,
        types::{PersistedLogin, Profile, SessionKind},
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
        }
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.starts_with("delete /api/v1/plugin/session http/1.1"));
        assert!(request.contains("authorization: bearer retained-upgrade-token"));
        let body = r#"{"error":"raw-retained-delete-failure"}"#;
        write!(
            stream,
            "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });

    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_str().unwrap().to_owned();
    let token = "retained-upgrade-token";
    let persisted = PersistedLogin {
        hub_url: hub_url.clone(),
        token: token.to_owned(),
        session_kind: SessionKind::Authenticated,
        profile: Profile {
            user_id: "retained-user".to_owned(),
            user_name: "Retained User".to_owned(),
            tenant_id: String::new(),
            tenant_name: String::new(),
            avatar: String::new(),
        },
    };
    persistence::store(&config_dir, &persisted).unwrap();
    std::fs::create_dir(
        directory
            .path()
            .join("pandar-plugin-pending-revocations.json"),
    )
    .unwrap();
    let login_bytes = std::fs::read(directory.path().join("pandar-plugin-login.json")).unwrap();

    let session = pandar_plugin_printer_refresh_session_create(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
    );
    let identity = pandar_plugin_account_identity_create();
    let account = Arc::new(RaceAccount {
        config_dir: config_dir.clone(),
        state: Mutex::new(RaceState {
            hub_url,
            token: token.to_owned(),
            account_epoch: 19,
            session_kind: SessionKind::Authenticated as i32,
        }),
        actions: Mutex::new(Vec::new()),
        block_clear: AtomicBool::new(false),
        clear_entered: (Mutex::new(false), Condvar::new()),
        release_clear: (Mutex::new(false), Condvar::new()),
        block_return: AtomicBool::new(true),
        return_entered: (Mutex::new(false), Condvar::new()),
        release_return: (Mutex::new(false), Condvar::new()),
    });
    let session_address = session as usize;
    let account_address = Arc::as_ptr(&account) as usize;
    let passive =
        thread::spawn(move || run_logout(session_address, identity, false, account_address));
    let entered = account.return_entered.0.lock().unwrap();
    drop(
        account
            .return_entered
            .1
            .wait_while(entered, |entered| !*entered)
            .unwrap(),
    );
    let requested =
        thread::spawn(move || run_logout(session_address, identity, true, account_address));
    crate::connection::ffi::session(session)
        .unwrap()
        .wait_for_account_logout_follower();
    *account.release_return.0.lock().unwrap() = true;
    account.release_return.1.notify_all();

    let passive = passive.join().unwrap();
    let requested = requested.join().unwrap();
    server.join().unwrap();
    for outcome in [passive, requested] {
        assert_eq!(outcome.status, 1);
        assert_eq!(outcome.http_code, 503);
        assert_eq!(outcome.body, r#"{"error":"invalid_response"}"#);
    }
    let state = account.state.lock().unwrap();
    assert!(state.token.is_empty());
    assert_eq!(state.account_epoch, 20);
    assert_eq!(state.session_kind, 0);
    drop(state);
    assert_eq!(
        std::fs::read(directory.path().join("pandar-plugin-login.json")).unwrap(),
        login_bytes
    );
    assert_eq!(
        *account.actions.lock().unwrap(),
        vec![
            (MUTATION_CLEAR, PluginAccountNotification::Logout as i32),
            (
                MUTATION_HTTP_ERROR,
                PluginAccountNotification::Silent as i32
            ),
        ]
    );
    assert!(
        directory
            .path()
            .join("pandar-plugin-direct-revocation.json")
            .is_file()
    );
    assert_eq!(persistence::load(&config_dir).unwrap(), None);
    pandar_plugin_printer_refresh_session_destroy(session);
}
