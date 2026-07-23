mod authenticated;
mod commit;
mod logout;
mod persisted;
mod recovery;
mod transaction;

#[cfg(test)]
mod tests;

use std::ffi::c_void;

use crate::{
    PluginHttpResult,
    cancellation::RequestCancellation,
    connection::{
        ffi::session,
        no_auth::{
            pandar_plugin_no_auth_retry_arm, pandar_plugin_no_auth_retry_begin,
            pandar_plugin_no_auth_retry_complete,
        },
        no_auth_rotation::{NoAuthRotationBegin, NoAuthRotationKey, NoAuthRotationOutcome},
    },
    pandar_plugin_create_no_auth_session,
    plugin_session::create_no_auth_session_with_cancellation,
    result, stable_error_body,
    studio_policy::{ACCOUNT_ACTION_APPLY, ACCOUNT_ACTION_LOGIN},
};

use commit::{Candidate, CommitMode, commit_candidate, initial_current};
use recovery::retry_pending_revocation_with_cancellation;
pub use transaction::PluginWithCurrentAccount;
use transaction::{AccountView, capture};

const ACCOUNT_EVENT_NONE: i32 = 0;
const ACCOUNT_EVENT_LOGIN: i32 = 1;

#[repr(C)]
pub struct PluginLifecycleResult {
    pub http: PluginHttpResult,
    pub account_event: i32,
    pub report_http_error: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct NoAuthExpected {
    pub(crate) hub_url: String,
    pub(crate) token: String,
    pub(crate) account_epoch: u64,
    pub(crate) config_epoch: u64,
    pub(crate) session_kind: i32,
}

#[derive(Debug)]
pub(crate) enum NoAuthRecovery {
    NotApplicable,
    Recovered(NoAuthExpected),
    Stale,
    Failed(NoAuthRotationOutcome),
}

impl NoAuthExpected {
    fn from_view(current: AccountView) -> Self {
        Self {
            hub_url: current.hub_url,
            token: current.token,
            account_epoch: current.account_epoch,
            config_epoch: current.config_epoch,
            session_kind: current.session_kind,
        }
    }

    fn with_token(&self, token: String) -> Self {
        Self {
            hub_url: self.hub_url.clone(),
            token,
            account_epoch: self.account_epoch,
            config_epoch: self.config_epoch,
            session_kind: self.session_kind,
        }
    }
}

pub(crate) fn current_expected(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> anyhow::Result<NoAuthExpected> {
    let current = capture(account_context, with_current)?;
    Ok(NoAuthExpected {
        hub_url: current.hub_url,
        token: current.token,
        account_epoch: current.account_epoch,
        config_epoch: current.config_epoch,
        session_kind: current.session_kind,
    })
}

pub(crate) fn recover(
    session_ptr: *mut c_void,
    expected: NoAuthExpected,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> NoAuthRecovery {
    recover_with_cancellation(
        session_ptr,
        expected,
        account_context,
        with_current,
        RequestCancellation::disabled(),
    )
}

pub(crate) fn recover_with_cancellation(
    session_ptr: *mut c_void,
    expected: NoAuthExpected,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    cancellation: RequestCancellation,
) -> NoAuthRecovery {
    let Some(session) = session(session_ptr) else {
        return NoAuthRecovery::Failed(stable_outcome("invalid_refresh_session"));
    };
    let current = match capture(account_context, with_current) {
        Ok(current) => current,
        Err(error) => return NoAuthRecovery::Failed(diagnosed_outcome(error)),
    };
    let config_dir = current.config_dir.clone();
    match refresh_action(&expected, &current) {
        ACCOUNT_ACTION_LOGIN => {
            return NoAuthRecovery::Recovered(NoAuthExpected::from_view(current));
        }
        ACCOUNT_ACTION_APPLY => {}
        _ if expected.session_kind == 2 => return NoAuthRecovery::Stale,
        _ => return NoAuthRecovery::NotApplicable,
    }
    let pending = retry_pending_revocation_with_cancellation(&config_dir, cancellation);
    if pending.status != 0 {
        return NoAuthRecovery::Failed(pending);
    }

    let key = NoAuthRotationKey::new(
        expected.hub_url.clone(),
        expected.token.clone(),
        expected.account_epoch,
        expected.config_epoch,
    );
    match session.begin_no_auth_rotation_cancellable(key.clone(), cancellation) {
        NoAuthRotationBegin::Finished(outcome) => {
            return recovery_from_outcome(outcome, &expected, account_context, with_current);
        }
        NoAuthRotationBegin::NotApplicable => return NoAuthRecovery::Stale,
        NoAuthRotationBegin::Cancelled => {
            return NoAuthRecovery::Failed(stable_outcome("request_cancelled"));
        }
        NoAuthRotationBegin::Started => {}
    }

    let response = take_http(create_no_auth_session_with_cancellation(
        &expected.hub_url,
        cancellation,
    ));
    let recovery = if response.status != 0 {
        NoAuthRecovery::Failed(response)
    } else {
        match Candidate::decode(&response.body) {
            Ok(candidate) => match commit_candidate(
                account_context,
                with_current,
                &expected,
                &candidate,
                CommitMode::Refresh,
            ) {
                Ok(true) => {
                    NoAuthRecovery::Recovered(expected.with_token(candidate.token().to_owned()))
                }
                Ok(false) => NoAuthRecovery::Stale,
                Err(error) => NoAuthRecovery::Failed(diagnosed_outcome(error)),
            },
            Err(error) => NoAuthRecovery::Failed(diagnosed_outcome(error)),
        }
    };
    let outcome = match &recovery {
        NoAuthRecovery::Recovered(_) => NoAuthRotationOutcome {
            status: 0,
            http_code: 200,
            body: String::new(),
        },
        NoAuthRecovery::Stale => stale_outcome(),
        NoAuthRecovery::Failed(outcome) => outcome.clone(),
        NoAuthRecovery::NotApplicable => stable_outcome("stale_no_auth_session"),
    };
    let finished = session.finish_no_auth_rotation(key, outcome);
    debug_assert!(finished);
    recovery
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_account_no_auth_bootstrap(
    session_ptr: *mut c_void,
    initial_attempt: bool,
    now_ms: u64,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let Some(session) = session(session_ptr) else {
        return lifecycle_failure(stable_outcome("invalid_refresh_session"), false);
    };
    if session.account_logout_in_flight() || session.no_auth_rotation_in_flight() {
        return lifecycle_none();
    }
    let expected = match capture(account_context, with_current) {
        Ok(current) => current,
        Err(error) => return lifecycle_failure(diagnosed_outcome(error), false),
    };
    if session.account_logout_in_flight() {
        return lifecycle_none();
    }
    if expected.transition_pending || !expected.token.is_empty() {
        retry_pending_revocation(&expected.config_dir);
        return lifecycle_none();
    }
    if initial_attempt {
        pandar_plugin_no_auth_retry_arm(session_ptr, now_ms);
    }
    let config_dir = expected.config_dir.clone();
    let expected = NoAuthExpected {
        hub_url: expected.hub_url,
        token: expected.token,
        account_epoch: expected.account_epoch,
        config_epoch: expected.config_epoch,
        session_kind: expected.session_kind,
    };
    let pending = retry_pending_revocation(&config_dir);
    if pending.status != 0 {
        return lifecycle_failure(pending, initial_attempt);
    }
    let retry_started = pandar_plugin_no_auth_retry_begin(session_ptr, now_ms) == 1;
    if !retry_started {
        return lifecycle_none();
    }
    if !initial_current(account_context, with_current, &expected) {
        pandar_plugin_no_auth_retry_complete(session_ptr, 1, now_ms);
        return lifecycle_none();
    }
    let response = take_http(pandar_plugin_create_no_auth_session(
        expected.hub_url.as_ptr(),
        expected.hub_url.len(),
    ));
    if response.status != 0 {
        pandar_plugin_no_auth_retry_complete(session_ptr, response.status, now_ms);
        let report = initial_attempt && initial_current(account_context, with_current, &expected);
        return lifecycle_failure(response, report);
    }
    let committed = Candidate::decode(&response.body).and_then(|candidate| {
        commit_candidate(
            account_context,
            with_current,
            &expected,
            &candidate,
            CommitMode::Initial,
        )
    });
    match committed {
        Ok(true) => {
            pandar_plugin_no_auth_retry_complete(session_ptr, 0, now_ms);
            PluginLifecycleResult {
                http: result(0, 200, ""),
                account_event: ACCOUNT_EVENT_LOGIN,
                report_http_error: 0,
            }
        }
        Ok(false) => {
            pandar_plugin_no_auth_retry_complete(session_ptr, 1, now_ms);
            lifecycle_none()
        }
        Err(error) => {
            pandar_plugin_no_auth_retry_complete(session_ptr, 1, now_ms);
            lifecycle_failure(diagnosed_outcome(error), false)
        }
    }
}

fn refresh_action(expected: &NoAuthExpected, current: &AccountView) -> i32 {
    crate::studio_policy::account_refresh::pandar_plugin_account_refresh_action(
        expected.account_epoch,
        current.account_epoch,
        expected.config_epoch,
        current.config_epoch,
        current.transition_pending,
        expected.session_kind,
        current.session_kind,
        expected.hub_url.as_ptr(),
        expected.hub_url.len(),
        current.hub_url.as_ptr(),
        current.hub_url.len(),
        expected.token.as_ptr(),
        expected.token.len(),
        current.token.as_ptr(),
        current.token.len(),
    )
}

pub(super) fn retry_pending_revocation(config_dir: &str) -> NoAuthRotationOutcome {
    retry_pending_revocation_with_cancellation(config_dir, RequestCancellation::disabled())
}

fn recovery_from_outcome(
    outcome: NoAuthRotationOutcome,
    expected: &NoAuthExpected,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> NoAuthRecovery {
    if outcome == stale_outcome() {
        return NoAuthRecovery::Stale;
    }
    if outcome.status != 0 {
        return NoAuthRecovery::Failed(outcome);
    }
    let current = match capture(account_context, with_current) {
        Ok(current) => current,
        Err(error) => return NoAuthRecovery::Failed(diagnosed_outcome(error)),
    };
    if refresh_action(expected, &current) == ACCOUNT_ACTION_LOGIN {
        NoAuthRecovery::Recovered(NoAuthExpected::from_view(current))
    } else {
        NoAuthRecovery::Stale
    }
}

pub(crate) fn take_http(response: PluginHttpResult) -> NoAuthRotationOutcome {
    let body = if response.body_ptr.is_null() || response.body_len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(unsafe {
            std::slice::from_raw_parts(response.body_ptr, response.body_len)
        })
        .into_owned()
    };
    crate::pandar_plugin_free_with_capacity(
        response.body_ptr.cast(),
        response.body_len,
        response.body_cap,
    );
    NoAuthRotationOutcome {
        status: response.status,
        http_code: response.http_code,
        body,
    }
}

pub(crate) fn into_http(outcome: NoAuthRotationOutcome) -> PluginHttpResult {
    result(outcome.status, outcome.http_code, outcome.body)
}

fn stable_outcome(error: &str) -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 0,
        body: stable_error_body(error),
    }
}

fn stale_outcome() -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code: 409,
        body: stable_error_body("stale_no_auth_session"),
    }
}

fn diagnosed_outcome(error: anyhow::Error) -> NoAuthRotationOutcome {
    eprintln!("pandar no-auth account lifecycle failed: {error:#}");
    stable_outcome("account_state_unavailable")
}

fn lifecycle_none() -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: result(0, 204, ""),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: 0,
    }
}

fn lifecycle_failure(outcome: NoAuthRotationOutcome, report: bool) -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: into_http(outcome),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: i32::from(report),
    }
}
