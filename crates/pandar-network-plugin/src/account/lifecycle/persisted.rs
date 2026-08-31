use std::ffi::c_void;

use anyhow::{Context, ensure};

use crate::{connection::no_auth_rotation::NoAuthRotationOutcome, stable_error_body};

use super::{
    ACCOUNT_EVENT_NONE, PluginLifecycleResult,
    authenticated::ExpectedAccount,
    into_http, take_http,
    transaction::{
        AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountView,
        PluginWithCurrentAccount, capture, transact,
    },
};
use crate::account::{
    persistence,
    runtime::canonical_hub_identity,
    types::{LocalServerConfig, PersistedLogin},
};

const MUTATION_REPLACE: i32 = 1;
const MUTATION_RUNTIME_HUB: i32 = 6;

struct LoadedAccount {
    token: String,
    user_id: String,
    user_name: String,
    avatar: String,
    profile_json: String,
    session_kind: i32,
}

struct LoadContext<'a> {
    expected: &'a ExpectedAccount,
    loaded: &'a LoadedAccount,
    state: ApplyState,
}

struct RuntimeContext<'a> {
    expected: &'a ExpectedAccount,
    hub_url: &'a str,
    state: ApplyState,
}

enum ApplyState {
    Pending,
    Applied,
    Stale,
    Failed(NoAuthRotationOutcome),
}

impl LoadedAccount {
    fn from_login(login: PersistedLogin) -> anyhow::Result<Self> {
        ensure!(
            !login.token.trim().is_empty(),
            "persisted Studio login has no token"
        );
        let profile_json =
            serde_json::to_string(&login.profile).context("encode persisted Studio profile")?;
        Ok(Self {
            token: login.token,
            user_id: login.profile.user_id,
            user_name: login.profile.user_name,
            avatar: login.profile.avatar,
            profile_json,
            session_kind: login.session_kind as i32,
        })
    }
}

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
    let expected = ExpectedAccount::from_view(&current);
    let login = match persistence::load(&current.config_dir) {
        Ok(Some(login)) => login,
        Ok(None) => return lifecycle(success(204)),
        Err(error) => return lifecycle(diagnosed(error)),
    };
    if canonical_hub_identity(&login.hub_url) != canonical_hub_identity(&current.hub_url) {
        return lifecycle(success(204));
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

unsafe extern "C" fn load_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<LoadContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = unsafe { AccountView::read(view) }?;
        if !context.expected.matches(&current) || !current.token.is_empty() {
            context.state = ApplyState::Stale;
            return Ok(());
        }
        let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
        mutation.action = MUTATION_REPLACE;
        mutation.token = PluginAccountBytes::from_str(&context.loaded.token);
        mutation.user_id = PluginAccountBytes::from_str(&context.loaded.user_id);
        mutation.user_name = PluginAccountBytes::from_str(&context.loaded.user_name);
        mutation.avatar = PluginAccountBytes::from_str(&context.loaded.avatar);
        mutation.profile_json = PluginAccountBytes::from_str(&context.loaded.profile_json);
        mutation.session_kind = context.loaded.session_kind;
        context.state = ApplyState::Applied;
        Ok(())
    })();
    transaction_status(work, &mut context.state)
}

unsafe extern "C" fn runtime_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<RuntimeContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = unsafe { AccountView::read(view) }?;
        if !context.expected.matches(&current) {
            context.state = ApplyState::Stale;
            return Ok(());
        }
        let local_failure = match persistence::clear(&current.config_dir) {
            Ok(durability) => durability
                .require_confirmed("durably clear Studio login after Hub change")
                .err()
                .map(diagnosed),
            Err(error) => Some(diagnosed(
                error.context("clear persisted Studio login after Hub change"),
            )),
        };
        let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
        mutation.action = MUTATION_RUNTIME_HUB;
        mutation.hub_url = PluginAccountBytes::from_str(context.hub_url);
        if let Some(failure) = &local_failure {
            mutation.error_body = PluginAccountBytes::from_str(&failure.body);
        }
        context.state = match local_failure {
            Some(failure) => ApplyState::Failed(failure),
            None => ApplyState::Applied,
        };
        Ok(())
    })();
    transaction_status(work, &mut context.state)
}

fn transaction_status(work: anyhow::Result<()>, state: &mut ApplyState) -> i32 {
    match work {
        Ok(()) => 0,
        Err(error) => {
            *state = ApplyState::Failed(diagnosed(error));
            1
        }
    }
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
