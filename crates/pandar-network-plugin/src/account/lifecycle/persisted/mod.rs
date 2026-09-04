use std::ffi::c_void;

use anyhow::Context;

use crate::{connection::no_auth_rotation::NoAuthRotationOutcome, stable_error_body};

use super::{
    ACCOUNT_EVENT_NONE, PluginLifecycleResult,
    authenticated::ExpectedAccount,
    into_http, take_http,
    transaction::{AccountView, PluginWithCurrentAccount, capture, transact},
};
use crate::account::{
    persistence, runtime,
    runtime::canonical_hub_identity,
    server_selection::{self, PersistedServerSelection},
    types::{LocalServerConfig, PendingRevocation, PersistedLogin, SessionKind},
};

use transactions::{
    ApplyState, LoadContext, LoadedAccount, RestoreServersContext, RuntimeContext,
    load_transaction, restore_servers_transaction, runtime_transaction,
};

mod transactions;

#[unsafe(no_mangle)]
/// # Safety
/// `account_context` and `with_current` must remain valid for this call and every synchronous
/// account transaction callback it invokes.
pub unsafe extern "C" fn pandar_plugin_account_load_persisted(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let current = match unsafe { capture(account_context, with_current) } {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    if !current.token.is_empty() {
        return lifecycle(success(204));
    }
    let current = match restore_saved_servers(account_context, with_current, current) {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    let expected = ExpectedAccount::from_view(&current);
    let login = match persistence::load(&current.config_dir) {
        Ok(Some(login)) => login,
        Ok(None) => return lifecycle(success(204)),
        Err(error) => return lifecycle(diagnosed(error)),
    };
    if canonical_hub_identity(&login.hub_url) != canonical_hub_identity(&current.hub_url) {
        return lifecycle(success(204));
    }
    if login.session_kind == SessionKind::Authenticated && login.profile.tenant_id.trim().is_empty()
    {
        // A pre-upgrade write replaced the canonical profile with Studio's
        // tenantless identity echo. Restoring it would look logged in while the
        // tenant printer cache can never start, so retire the credential and
        // leave Studio on the normal sign-in flow.
        return lifecycle(clear_tenantless_login(&current.config_dir, &login));
    }
    let loaded = match LoadedAccount::from_login(login) {
        Ok(loaded) => loaded,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    let mut context = LoadContext {
        expected: &expected,
        loaded: &loaded,
        state: ApplyState::Pending,
    };
    if let Err(error) = unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut LoadContext<'_>).cast(),
            load_transaction,
        )
    } {
        return lifecycle(diagnosed(error));
    }
    finish(context.state)
}

/// Removes a tenantless persisted login only while the file still holds that
/// exact credential, so a login committed concurrently is never discarded.
fn clear_tenantless_login(config_dir: &str, login: &PersistedLogin) -> NoAuthRotationOutcome {
    let cleared = persistence::clear_matching(
        config_dir,
        &PendingRevocation {
            hub_url: login.hub_url.clone(),
            token: login.token.clone(),
        },
    )
    .and_then(|durability| durability.require_confirmed("durably clear tenantless Studio login"));
    match cleared {
        Ok(()) => success(204),
        Err(error) => diagnosed(error.context("clear tenantless persisted Studio login")),
    }
}

/// Restores a manually selected Web URL and its discovered canonical Hub identity before
/// the persisted Studio login is evaluated. Explicit plugin URL environment configuration
/// outranks the saved selection, and an unreadable or malformed selection fails closed.
fn restore_saved_servers(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    current: AccountView,
) -> anyhow::Result<AccountView> {
    if current.config_dir.is_empty() || runtime::url_environment_configured() {
        return Ok(current);
    }
    let selection = match server_selection::load(&current.config_dir) {
        Ok(Some(selection)) => selection,
        Ok(None) => return Ok(current),
        Err(error) => {
            eprintln!("pandar saved server selection ignored: {error:#}");
            return Ok(current);
        }
    };
    let Some(selection) =
        PersistedServerSelection::new(selection.web_url, selection.hub_url.clone())
    else {
        eprintln!("pandar saved server selection ignored: selection does not form canonical URLs");
        return Ok(current);
    };
    let current_frontend = crate::normalize_hub_url(current.frontend_url.clone())
        .unwrap_or_else(|| current.frontend_url.clone());
    if selection.hub_url == canonical_hub_identity(&current.hub_url)
        && selection.web_url == current_frontend
    {
        return Ok(current);
    }
    let expected = ExpectedAccount::from_view(&current);
    let mut context = RestoreServersContext {
        expected: &expected,
        hub_url: &selection.hub_url,
        frontend_url: &selection.web_url,
        state: ApplyState::Pending,
    };
    unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut RestoreServersContext<'_>).cast(),
            restore_servers_transaction,
        )?;
    }
    match context.state {
        ApplyState::Applied => unsafe { capture(account_context, with_current) },
        _ => Ok(current),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `account_context` and `with_current` must remain valid for this call and every synchronous
/// account transaction callback it invokes.
pub unsafe extern "C" fn pandar_plugin_account_refresh_runtime(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let response = take_http(crate::pandar_plugin_local_webserver_config());
    if response.status != 0 {
        return lifecycle(response);
    }
    let config: LocalServerConfig = match serde_json::from_str(&response.body) {
        Ok(config) => config,
        Err(error) => return lifecycle(diagnosed(error.into())),
    };
    let hub_url = canonical_hub_identity(&config.hub_url);
    if hub_url.is_empty() {
        return lifecycle(stable_failure("account_state_unavailable"));
    }
    let current = match unsafe { capture(account_context, with_current) } {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    if config.user_selected
        && let Some(selection) =
            PersistedServerSelection::new(config.web_url.clone(), hub_url.clone())
        && let Err(error) = persist_selection(&current.config_dir, &selection)
    {
        return lifecycle(diagnosed(error));
    }
    if current.hub_url == hub_url {
        return lifecycle(success(204));
    }
    let expected = ExpectedAccount::from_view(&current);
    let mut context = RuntimeContext {
        expected: &expected,
        hub_url: &hub_url,
        state: ApplyState::Pending,
    };
    if let Err(error) = unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut RuntimeContext<'_>).cast(),
            runtime_transaction,
        )
    } {
        return lifecycle(diagnosed(error));
    }
    finish(context.state)
}

/// Durably records a manually selected target server. The write is skipped when the
/// saved selection already matches, and durability must be confirmed before the
/// runtime Hub switch proceeds.
fn persist_selection(config_dir: &str, selection: &PersistedServerSelection) -> anyhow::Result<()> {
    if config_dir.is_empty() {
        return Ok(());
    }
    let unchanged = match server_selection::load(config_dir) {
        Ok(Some(saved)) => &saved == selection,
        // A missing or malformed selection is rewritten rather than trusted.
        _ => false,
    };
    if unchanged {
        return Ok(());
    }
    server_selection::store(config_dir, selection)?
        .require_confirmed("durably persist selected Pandar server")
        .context("persist selected Pandar server")
}

fn finish(state: ApplyState) -> PluginLifecycleResult {
    match state {
        ApplyState::Applied | ApplyState::Stale => lifecycle(success(200)),
        ApplyState::Failed(failure) => lifecycle(failure),
        ApplyState::Pending => lifecycle(stable_failure("account_state_unavailable")),
    }
}

fn success(http_code: u32) -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 0,
        http_code,
        body: String::new(),
    }
}

fn stable_failure(error: &str) -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 0,
        body: stable_error_body(error),
    }
}

fn diagnosed(error: anyhow::Error) -> NoAuthRotationOutcome {
    eprintln!("pandar persisted account lifecycle failed: {error:#}");
    stable_failure("account_state_unavailable")
}

fn lifecycle(outcome: NoAuthRotationOutcome) -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: into_http(outcome),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: 0,
    }
}
