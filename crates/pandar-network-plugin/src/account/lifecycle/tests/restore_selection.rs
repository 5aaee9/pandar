use std::{ffi::c_void, sync::Mutex};

use crate::account::lifecycle::transaction::{
    MUTATION_REPLACE, MUTATION_RUNTIME_SERVERS, PluginAccountBytes, PluginAccountMutation,
    PluginAccountNotification, PluginAccountTransaction, PluginAccountView,
};
use crate::account::{
    persistence,
    server_selection::{self, PersistedServerSelection},
    types::{PersistedLogin, Profile, SessionKind},
};

const URL_VARIABLES: [&str; 4] = [
    "PANDAR_PLUGIN_HUB_URL",
    "APP_API_URL",
    "PANDAR_PLUGIN_FRONTEND_URL",
    "APP_BASE_URL",
];

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct ScrubbedEnvironment {
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Vec<(&'static str, Option<String>)>,
}

impl Drop for ScrubbedEnvironment {
    fn drop(&mut self) {
        for (name, value) in self.previous.drain(..) {
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

fn without_url_environment() -> ScrubbedEnvironment {
    let guard = ENV_LOCK.lock().unwrap();
    let previous = URL_VARIABLES
        .iter()
        .map(|name| {
            let value = std::env::var(name).ok();
            unsafe { std::env::remove_var(name) };
            (*name, value)
        })
        .collect();
    ScrubbedEnvironment {
        _guard: guard,
        previous,
    }
}

fn with_hub_environment(value: &str) -> ScrubbedEnvironment {
    let scrubbed = without_url_environment();
    unsafe { std::env::set_var("PANDAR_PLUGIN_HUB_URL", value) };
    scrubbed
}

struct RestoreAccount {
    config_dir: String,
    hub_url: String,
    frontend_url: String,
    token: String,
    account_epoch: u64,
    actions: Vec<i32>,
    applied_hubs: Vec<String>,
    applied_frontends: Vec<String>,
}

unsafe extern "C" fn with_restore_account(
    opaque: *mut c_void,
    context: *mut c_void,
    transaction: Option<PluginAccountTransaction>,
) -> i32 {
    let account = unsafe { &mut *opaque.cast::<RestoreAccount>() };
    let empty = PluginAccountBytes::from_str("");
    let view = PluginAccountView {
        config_dir: PluginAccountBytes::from_str(&account.config_dir),
        hub_url: PluginAccountBytes::from_str(&account.hub_url),
        frontend_url: PluginAccountBytes::from_str(&account.frontend_url),
        token: PluginAccountBytes::from_str(&account.token),
        user_id: empty,
        user_name: empty,
        avatar: empty,
        profile_json: empty,
        account_epoch: account.account_epoch,
        config_epoch: 3,
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
    if status == 0 && mutation.action != 0 {
        // Mirror the shim bridge application so later captures observe the mutation.
        match mutation.action {
            MUTATION_RUNTIME_SERVERS => {
                account.hub_url = unsafe { mutation.hub_url.read("hub") }.expect("runtime hub");
                account.frontend_url =
                    unsafe { mutation.frontend_url.read("frontend") }.expect("frontend");
                account.applied_hubs.push(account.hub_url.clone());
                account.applied_frontends.push(account.frontend_url.clone());
                account.account_epoch = account.account_epoch.wrapping_add(1);
                account.token.clear();
            }
            MUTATION_REPLACE => {
                account.token = unsafe { mutation.token.read("token") }.expect("token");
            }
            _ => {}
        }
        account.actions.push(mutation.action);
    }
    status
}

fn profile() -> Profile {
    Profile {
        user_id: "restore-user".to_owned(),
        user_name: "Restore User".to_owned(),
        tenant_id: "tenant-1".to_owned(),
        tenant_name: "Tenant".to_owned(),
        avatar: String::new(),
    }
}

fn store_state(config_dir: &str, selection: Option<(&str, &str)>, login_hub: &str) {
    if let Some((web_url, hub_url)) = selection {
        let selection = PersistedServerSelection::new(web_url.to_owned(), hub_url.to_owned())
            .expect("test selection is canonical");
        server_selection::store(config_dir, &selection)
            .unwrap()
            .require_confirmed("test selection")
            .unwrap();
    }
    persistence::store(
        config_dir,
        &PersistedLogin {
            hub_url: login_hub.to_owned(),
            token: "restore-token".to_owned(),
            session_kind: SessionKind::Authenticated,
            profile: profile(),
        },
    )
    .unwrap()
    .require_confirmed("test login")
    .unwrap();
}

fn restore_account(config_dir: &str) -> RestoreAccount {
    RestoreAccount {
        config_dir: config_dir.to_owned(),
        hub_url: "http://127.0.0.1:8080".to_owned(),
        frontend_url: "http://localhost:3000".to_owned(),
        token: String::new(),
        account_epoch: 7,
        actions: Vec::new(),
        applied_hubs: Vec::new(),
        applied_frontends: Vec::new(),
    }
}

fn load_persisted(account: &mut RestoreAccount) -> crate::PluginHttpResult {
    let result = unsafe {
        super::super::persisted::pandar_plugin_account_load_persisted(
            (account as *mut RestoreAccount).cast(),
            Some(with_restore_account),
        )
    };
    result.http
}

#[test]
fn saved_selection_restores_servers_before_same_hub_login_evaluation() {
    let _environment = without_url_environment();
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    store_state(
        &config_dir,
        Some((
            "https://pandar-web.example.test",
            "https://pandar-hub.example.test",
        )),
        "https://pandar-hub.example.test",
    );
    let mut account = restore_account(&config_dir);

    let http = load_persisted(&mut account);

    assert_eq!(http.status, 0);
    assert_eq!(http.http_code, 200);
    assert_eq!(
        account.actions,
        [MUTATION_RUNTIME_SERVERS, MUTATION_REPLACE]
    );
    assert_eq!(account.hub_url, "https://pandar-hub.example.test");
    assert_eq!(account.frontend_url, "https://pandar-web.example.test");
    assert_eq!(account.token, "restore-token");
}

#[test]
fn tenantless_persisted_authenticated_login_is_retired_not_restored() {
    let _environment = without_url_environment();
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    persistence::store(
        &config_dir,
        &PersistedLogin {
            hub_url: "http://127.0.0.1:8080".to_owned(),
            token: "tenantless-token".to_owned(),
            session_kind: SessionKind::Authenticated,
            profile: Profile {
                user_id: "restore-user".to_owned(),
                user_name: "Restore User".to_owned(),
                tenant_id: String::new(),
                tenant_name: String::new(),
                avatar: String::new(),
            },
        },
    )
    .unwrap()
    .require_confirmed("test login")
    .unwrap();
    let mut account = restore_account(&config_dir);

    let http = load_persisted(&mut account);

    assert_eq!(http.status, 0);
    assert_eq!(
        http.http_code, 204,
        "a tenantless session must not restore as logged in"
    );
    assert!(
        account.actions.is_empty(),
        "tenantless login must not apply a session mutation: {:?}",
        account.actions
    );
    assert!(account.token.is_empty());
    assert!(
        !directory.path().join("pandar-plugin-login.json").exists(),
        "retired credential must be cleaned up"
    );
}

#[test]
fn saved_selection_never_restores_a_foreign_hub_credential() {
    let _environment = without_url_environment();
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    store_state(
        &config_dir,
        Some((
            "https://pandar-web.example.test",
            "https://pandar-hub.example.test",
        )),
        "https://another-hub.example.test",
    );
    let mut account = restore_account(&config_dir);

    let http = load_persisted(&mut account);

    assert_eq!(http.status, 0);
    assert_eq!(http.http_code, 204);
    assert_eq!(account.actions, [MUTATION_RUNTIME_SERVERS]);
    assert_eq!(account.hub_url, "https://pandar-hub.example.test");
    assert!(account.token.is_empty());
}

#[test]
fn explicit_url_environment_outranks_the_saved_selection() {
    let _environment = with_hub_environment("https://operator-hub.example.test");
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    store_state(
        &config_dir,
        Some((
            "https://pandar-web.example.test",
            "https://pandar-hub.example.test",
        )),
        "https://pandar-hub.example.test",
    );
    let mut account = restore_account(&config_dir);

    let http = load_persisted(&mut account);

    assert_eq!(http.status, 0);
    assert_eq!(http.http_code, 204);
    assert!(account.actions.is_empty());
    assert_eq!(account.hub_url, "http://127.0.0.1:8080");
    assert!(account.token.is_empty());
}

#[test]
fn malformed_saved_selection_fails_closed_and_keeps_default_servers() {
    let _environment = without_url_environment();
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().to_string_lossy().into_owned();
    std::fs::write(
        directory.path().join("pandar-plugin-server-selection.json"),
        "{\"web_url\":",
    )
    .unwrap();
    store_state(&config_dir, None, "http://127.0.0.1:8080");
    let mut account = restore_account(&config_dir);

    let http = load_persisted(&mut account);

    assert_eq!(http.status, 0);
    assert_eq!(http.http_code, 200);
    assert_eq!(account.actions, [MUTATION_REPLACE]);
    assert_eq!(account.hub_url, "http://127.0.0.1:8080");
    assert_eq!(account.frontend_url, "http://localhost:3000");
    assert_eq!(account.token, "restore-token");
}
