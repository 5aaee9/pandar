use std::ffi::c_void;

use anyhow::Context;

use crate::{
    RequestKind,
    connection::{AccountLogoutBegin, ffi::session, no_auth_rotation::NoAuthRotationOutcome},
    http, result, stable_error_body,
    studio_policy::{
        ACCOUNT_ACTION_FAILURE, ACCOUNT_ACTION_LOGOUT, ACCOUNT_ACTION_NONE,
        login_observation::pandar_plugin_account_logout_action,
    },
};

use super::{
    ACCOUNT_EVENT_NONE, PluginLifecycleResult, into_http, take_http,
    transaction::{
        AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountNotification,
        PluginAccountView, PluginWithCurrentAccount, transact,
    },
};
use crate::account::{persistence, revocation, types::PendingRevocation};

#[cfg(test)]
#[path = "logout/external_race_tests.rs"]
mod external_race_tests;
mod retained;
#[cfg(test)]
#[path = "logout/tests.rs"]
mod tests;
mod unstaged;

use retained::{RetainedLogout, finish_retained};
use unstaged::{RevocationStage, UnstagedLogout, finish_unstaged, stage_revocation};

const MUTATION_CLEAR: i32 = 2;
const MUTATION_HTTP_ERROR: i32 = 3;

struct LogoutContext {
    identity: u64,
    request: bool,
    state: LogoutState,
}

enum LogoutState {
    Pending,
    None(String),
    Cleared(LogoutWork),
    Retained(RetainedLogout),
    Unstaged(UnstagedLogout),
    Failed(NoAuthRotationOutcome),
}

struct LogoutWork {
    expected: LoggedOutExpected,
    config_dir: String,
    revocation: Option<LogoutRevocation>,
    local_failure: Option<NoAuthRotationOutcome>,
}

#[derive(Clone)]
struct LogoutRevocation {
    candidate: PendingRevocation,
    stage: RevocationStage,
}

pub(super) struct LoggedOutExpected {
    pub(super) hub_url: String,
    pub(super) account_epoch: u64,
    pub(super) config_epoch: u64,
}

struct ReportContext<'a> {
    expected: &'a LoggedOutExpected,
    failure: &'a NoAuthRotationOutcome,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_account_logout(
    session_ptr: *mut c_void,
    identity: u64,
    request: bool,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let Some(session) = session(session_ptr) else {
        return failure_result(stable_outcome("invalid_refresh_session"));
    };
    let mut owner = match session.begin_account_logout(request) {
        AccountLogoutBegin::Owner(owner) => owner,
        AccountLogoutBegin::Follower(follower) => return outcome_result(follower.wait()),
        AccountLogoutBegin::Immediate => return success_result(),
    };
    owner.begin_finalization();
    let mut context = LogoutContext {
        identity,
        request,
        state: LogoutState::Pending,
    };
    if let Err(error) = transact(
        account_context,
        with_current,
        (&mut context as *mut LogoutContext).cast(),
        logout_transaction,
    ) {
        context.state = LogoutState::Failed(diagnosed_outcome(error));
    }
    let request = owner.seal_finalization();
    if request
        && !context.request
        && matches!(context.state, LogoutState::None(_) | LogoutState::Failed(_))
    {
        context.request = true;
        context.state = LogoutState::Pending;
        if let Err(error) = transact(
            account_context,
            with_current,
            (&mut context as *mut LogoutContext).cast(),
            logout_transaction,
        ) {
            context.state = LogoutState::Failed(diagnosed_outcome(error));
        }
    }
    let outcome = match context.state {
        LogoutState::Pending => stable_outcome("account_state_unavailable"),
        LogoutState::Failed(outcome) => outcome,
        LogoutState::None(config_dir) => {
            if request {
                revoke_pending(&config_dir)
            } else {
                success_outcome()
            }
        }
        LogoutState::Cleared(work) => finish_logout(account_context, with_current, work, request),
        LogoutState::Retained(work) => {
            finish_retained(account_context, with_current, work, request)
        }
        LogoutState::Unstaged(work) => finish_unstaged(account_context, with_current, work),
    };
    owner.complete(request, outcome.clone());
    outcome_result(outcome)
}

unsafe extern "C" fn logout_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<LogoutContext>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        let action = pandar_plugin_account_logout_action(
            context.identity,
            context.request,
            current.account_epoch,
            current.token.as_ptr(),
            current.token.len(),
        );
        match action {
            ACCOUNT_ACTION_FAILURE => {
                context.state = LogoutState::Failed(stable_outcome("account_state_unavailable"));
            }
            ACCOUNT_ACTION_NONE => {
                context.state = LogoutState::None(current.config_dir);
            }
            ACCOUNT_ACTION_LOGOUT => {
                if !context.request && !current.token.is_empty() {
                    let mutation = unsafe { mutation.as_mut() }
                        .context("account logout mutation is missing")?;
                    mutation.action = MUTATION_CLEAR;
                    mutation.notification = PluginAccountNotification::Logout;
                    context.state = LogoutState::Retained(RetainedLogout::new(&current));
                    return Ok(());
                }
                let mut revocation = None;
                if !current.token.is_empty() {
                    let candidate = PendingRevocation {
                        hub_url: current.hub_url.clone(),
                        token: current.token.clone(),
                    };
                    let stage = stage_revocation(&current.config_dir, &candidate);
                    if matches!(stage, RevocationStage::Failed) {
                        context.state =
                            LogoutState::Unstaged(UnstagedLogout::new(&current, candidate));
                        return Ok(());
                    }
                    revocation = Some(LogoutRevocation { candidate, stage });
                }
                let local_failure = match persistence::clear(&current.config_dir) {
                    Ok(durability) => durability
                        .require_confirmed("durably clear Studio login during logout")
                        .err()
                        .map(diagnosed_outcome),
                    Err(error) => Some(diagnosed_outcome(
                        error.context("clear persisted Studio login"),
                    )),
                };
                if !context.request
                    && let Some(failure) = local_failure
                {
                    context.state = LogoutState::Failed(failure);
                    return Ok(());
                }
                let mutation =
                    unsafe { mutation.as_mut() }.context("account logout mutation is missing")?;
                mutation.action = MUTATION_CLEAR;
                mutation.notification = PluginAccountNotification::Silent;
                if !current.token.is_empty() {
                    mutation.notification = PluginAccountNotification::Logout;
                }
                if let Some(failure) = &local_failure {
                    mutation.error_body = PluginAccountBytes::from_str(&failure.body);
                }
                context.state = LogoutState::Cleared(LogoutWork {
                    expected: LoggedOutExpected {
                        hub_url: current.hub_url,
                        account_epoch: current.account_epoch.wrapping_add(1),
                        config_epoch: current.config_epoch,
                    },
                    config_dir: current.config_dir,
                    revocation,
                    local_failure,
                });
            }
            _ => context.state = LogoutState::Failed(stable_outcome("account_state_unavailable")),
        }
        Ok(())
    })();
    match work {
        Ok(()) => 0,
        Err(error) => {
            context.state = LogoutState::Failed(diagnosed_outcome(error));
            1
        }
    }
}

fn finish_logout(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    work: LogoutWork,
    request: bool,
) -> NoAuthRotationOutcome {
    if let Some(local_failure) = work.local_failure {
        return local_failure;
    }
    let remote = if !request {
        success_outcome()
    } else if let Some(revocation) = work.revocation.clone() {
        match revocation.stage {
            RevocationStage::Staged => revoke_staged(&work.config_dir, revocation.candidate),
            RevocationStage::Failed => revoke_unstaged(revocation.candidate),
        }
    } else {
        revoke_pending(&work.config_dir)
    };
    if remote.status != 0 {
        report_remote_failure(account_context, with_current, &work.expected, &remote);
    }
    remote
}

pub(super) fn revoke_staged(
    config_dir: &str,
    revocation: PendingRevocation,
) -> NoAuthRotationOutcome {
    match revocation::revoke(config_dir, revocation) {
        Ok(Some(response)) => take_http(response),
        Ok(None) => success_outcome(),
        Err(error) => diagnosed_outcome(error.context("revoke staged plugin session")),
    }
}

pub(super) fn revoke_unstaged(revocation: PendingRevocation) -> NoAuthRotationOutcome {
    let response = http::delete_session(
        &format!("{}/api/v1/plugin/session", revocation.hub_url),
        &revocation.token,
        RequestKind::PluginSession,
    );
    if response.status == 0 || matches!(response.http_code, 401 | 410) {
        crate::pandar_plugin_free_with_capacity(
            response.body_ptr.cast(),
            response.body_len,
            response.body_cap,
        );
        success_outcome()
    } else {
        take_http(response)
    }
}

fn revoke_pending(config_dir: &str) -> NoAuthRotationOutcome {
    take_http(revocation::pandar_plugin_account_revoke_pending(
        config_dir.as_ptr(),
        config_dir.len(),
    ))
}

pub(super) fn report_remote_failure(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &LoggedOutExpected,
    failure: &NoAuthRotationOutcome,
) {
    let mut report = ReportContext { expected, failure };
    if let Err(error) = transact(
        account_context,
        with_current,
        (&mut report as *mut ReportContext<'_>).cast(),
        report_transaction,
    ) {
        eprintln!("pandar account logout failure delivery failed: {error:#}");
    }
}

unsafe extern "C" fn report_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<ReportContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        if current.account_epoch == context.expected.account_epoch
            && current.config_epoch == context.expected.config_epoch
            && current.hub_url == context.expected.hub_url
            && current.token.is_empty()
            && current.session_kind == 0
            && !current.transition_pending
        {
            let mutation =
                unsafe { mutation.as_mut() }.context("account HTTP error mutation is missing")?;
            mutation.action = MUTATION_HTTP_ERROR;
            mutation.error_body = PluginAccountBytes::from_str(&context.failure.body);
            mutation.http_code = context.failure.http_code;
        }
        Ok(())
    })();
    if let Err(error) = work {
        eprintln!("pandar account logout failure delivery failed: {error:#}");
        return 1;
    }
    0
}

fn outcome_result(outcome: NoAuthRotationOutcome) -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: into_http(outcome),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: 0,
    }
}

fn success_result() -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: result(0, 204, ""),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: 0,
    }
}

fn failure_result(outcome: NoAuthRotationOutcome) -> PluginLifecycleResult {
    outcome_result(outcome)
}

pub(super) fn success_outcome() -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 0,
        http_code: 204,
        body: String::new(),
    }
}

fn stable_outcome(error: &str) -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 0,
        body: stable_error_body(error),
    }
}

pub(super) fn diagnosed_outcome(error: anyhow::Error) -> NoAuthRotationOutcome {
    eprintln!("pandar account logout lifecycle failed: {error:#}");
    stable_outcome("account_state_unavailable")
}
