use std::ffi::c_void;

use super::super::{authenticated, take_http};
use crate::account::lifecycle::transaction::{
    MUTATION_FIRMWARE_FENCE, MUTATION_LOGIN, PluginAccountBytes, PluginAccountMutation,
    PluginAccountNotification, PluginAccountTransaction, PluginAccountView,
};
use crate::connection::no_auth_rotation::NoAuthRotationOutcome;

struct ProfileAccountState {
    config_dir: String,
    hub_url: String,
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
        hub_url: PluginAccountBytes::from_str(&account.hub_url),
        frontend_url: empty,
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
    let status =
        unsafe { (transaction.expect("account transaction"))(context, &view, &mut mutation) };
    if status == 0 && mutation.action == MUTATION_LOGIN {
        // Mirror the shim bridge application so later captures observe the login.
        account.token = unsafe { mutation.token.read("login token") }.expect("login token");
        account.user_id = unsafe { mutation.user_id.read("login user id") }.expect("login user id");
        account.user_name =
            unsafe { mutation.user_name.read("login user name") }.expect("login user name");
        account.avatar = unsafe { mutation.avatar.read("login avatar") }.expect("login avatar");
        account.profile_json =
            unsafe { mutation.profile_json.read("login profile") }.expect("login profile");
    }
    account.mutation_actions.push(mutation.action);
    status
}

fn profile_state(directory: &tempfile::TempDir, token: &str, user_id: &str) -> ProfileAccountState {
    ProfileAccountState {
        config_dir: directory.path().to_str().unwrap().to_owned(),
        hub_url: "http://hub".to_owned(),
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

fn exchange_ticket(account: &mut ProfileAccountState, ticket: &str) -> NoAuthRotationOutcome {
    let result = unsafe {
        authenticated::pandar_plugin_account_exchange_ticket(
            ticket.as_ptr(),
            ticket.len(),
            (account as *mut ProfileAccountState).cast(),
            Some(profile_account),
        )
    };
    take_http(result.http)
}

/// Serves exactly one Hub-shaped ticket-exchange response and returns its base URL.
fn one_shot_exchange_server(response_body: &'static str) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
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
fn studio_ticket_login_sequence_preserves_the_canonical_tenant_profile() {
    let exchange_body = r#"{"token":"account-b-token","expires_at":"2027-01-01T00:00:00Z","profile":{"user_id":"account-b","user_name":"Account B [pandar]","tenant_id":"tenant-b","tenant_name":"Tenant B"}}"#;
    let hub_url = one_shot_exchange_server(exchange_body);
    let directory = tempfile::tempdir().unwrap();
    let mut account = profile_state(&directory, "", "");
    account.hub_url = hub_url;

    // get_my_token: the Hub exchange commits the canonical profile.
    let token_response = exchange_ticket(&mut account, "login-ticket");
    assert_eq!(token_response.status, 0);
    assert!(token_response.body.contains("account-b-token"));
    assert_eq!(
        account.mutation_actions,
        vec![0, MUTATION_FIRMWARE_FENCE, MUTATION_LOGIN]
    );
    assert_eq!(account.token, "account-b-token");
    assert!(account.profile_json.contains("tenant-b"));

    // get_my_profile reports the committed identity back to Studio.
    let profile_response = get_profile(&mut account, "account-b-token");
    assert_eq!(profile_response.status, 0);
    assert!(profile_response.body.contains("account-b"));

    // change_user with Studio's native login envelope confirms the login.
    let envelope = r#"{"data":{"refresh_token":"refresh-1","token":"account-b-token","expires_in":"31536000","refresh_expires_in":"31536000","user":{"uid":"account-b","name":"Account B [pandar]","account":"Account B [pandar]","avatar":"avatar-b"}}}"#;
    let confirm_response = change_user(&mut account, envelope);
    assert_eq!(confirm_response.status, 0);
    assert_eq!(confirm_response.http_code, 200);
    assert_eq!(
        account.mutation_actions,
        // exchange capture, firmware fence, login commit, profile capture, confirmation capture
        vec![0, MUTATION_FIRMWARE_FENCE, MUTATION_LOGIN, 0, 0],
        "confirmation must not apply another mutation"
    );
    assert!(account.profile_json.contains("tenant-b"));

    let persisted =
        std::fs::read_to_string(directory.path().join("pandar-plugin-login.json")).unwrap();
    assert!(persisted.contains("tenant-b"), "{persisted}");
    assert!(persisted.contains("account-b-token"), "{persisted}");
}

#[test]
fn mismatched_studio_login_envelope_cannot_confirm_another_session() {
    let directory = tempfile::tempdir().unwrap();
    let mut account = profile_state(&directory, "account-b-token", "account-b");

    let wrong_token = change_user(
        &mut account,
        r#"{"data":{"token":"account-a-token","user":{"uid":"account-b","name":"Account B","account":"Account B","avatar":"avatar-b"}}}"#,
    );
    assert_eq!(wrong_token.status, 1);
    assert_eq!(wrong_token.http_code, 409);
    assert!(wrong_token.body.contains("stale_account_response"));

    let wrong_identity = change_user(
        &mut account,
        r#"{"data":{"token":"account-b-token","user":{"uid":"account-a","name":"Account A","account":"Account A","avatar":"avatar-a"}}}"#,
    );
    assert_eq!(wrong_identity.status, 1);
    assert_eq!(wrong_identity.http_code, 409);

    assert_eq!(account.mutation_actions, vec![0, 0]);
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        0,
        "mismatched envelopes must not persist state"
    );
}

#[test]
fn token_bearing_profile_change_user_still_commits_a_login() {
    let directory = tempfile::tempdir().unwrap();
    let mut account = profile_state(&directory, "", "");
    let response = change_user(
        &mut account,
        r#"{"token":"account-c-token","user_id":"account-c","user_name":"Account C","tenant_id":"tenant-c","tenant_name":"Tenant C"}"#,
    );
    assert_eq!(response.status, 0);
    assert_eq!(account.mutation_actions, vec![0, MUTATION_LOGIN]);
    assert_eq!(account.token, "account-c-token");
    assert!(account.profile_json.contains("tenant-c"));
    let persisted =
        std::fs::read_to_string(directory.path().join("pandar-plugin-login.json")).unwrap();
    assert!(persisted.contains("tenant-c"), "{persisted}");
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
