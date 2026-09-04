use std::ffi::c_void;

use anyhow::{Context, ensure};

use crate::{
    connection::no_auth_rotation::NoAuthRotationOutcome, pandar_plugin_exchange_ticket, read_utf8,
    stable_error_body,
};

use super::{
    ACCOUNT_EVENT_NONE, PluginLifecycleResult, into_http, take_http,
    transaction::{AccountView, PluginWithCurrentAccount, capture},
};
use crate::account::types::{
    AccountChange, Profile, SessionInput, StudioProfile, StudioToken, parse_account_change,
};

mod commit;

use commit::{CommitState, commit_login, fence_firmware, report_failure, revoke_ticket_candidate};

#[derive(Clone)]
pub(super) struct ExpectedAccount {
    pub(super) config_dir: String,
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) account_epoch: u64,
    pub(super) config_epoch: u64,
    pub(super) session_kind: i32,
}

struct Candidate {
    token: String,
    profile: Profile,
    profile_json: String,
}

impl ExpectedAccount {
    pub(super) fn from_view(view: &AccountView) -> Self {
        Self {
            config_dir: view.config_dir.clone(),
            hub_url: view.hub_url.clone(),
            token: view.token.clone(),
            account_epoch: view.account_epoch,
            config_epoch: view.config_epoch,
            session_kind: view.session_kind,
        }
    }

    pub(super) fn matches(&self, current: &AccountView) -> bool {
        !current.transition_pending
            && current.config_dir == self.config_dir
            && current.hub_url == self.hub_url
            && current.token == self.token
            && current.account_epoch == self.account_epoch
            && current.config_epoch == self.config_epoch
            && current.session_kind == self.session_kind
    }
}

impl Candidate {
    fn from_session(body: &str) -> anyhow::Result<Self> {
        let input: SessionInput =
            serde_json::from_str(body).context("decode typed authenticated account session")?;
        ensure!(
            !input.token.trim().is_empty(),
            "account session has no token"
        );
        let profile = input.profile.normalize()?;
        Self::new(input.token, profile)
    }

    fn new(token: String, profile: Profile) -> anyhow::Result<Self> {
        let profile_json =
            serde_json::to_string(&profile).context("encode canonical authenticated profile")?;
        Ok(Self {
            token,
            profile,
            profile_json,
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session_ptr` must identify a live account lifecycle session, `user_info_ptr` must be valid for
/// `user_info_len`, and `account_context` plus `with_current` must remain valid for every callback
/// made during this synchronous call.
pub unsafe extern "C" fn pandar_plugin_account_change_user(
    session_ptr: *mut c_void,
    identity: u64,
    user_info_ptr: *const u8,
    user_info_len: usize,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let Some(user_info) = (unsafe { read_utf8(user_info_ptr, user_info_len) }) else {
        return lifecycle(stable_failure("account_state_unavailable", 0));
    };
    if user_info.is_empty() || user_info == "{}" {
        return unsafe {
            super::logout::pandar_plugin_account_logout(
                session_ptr,
                identity,
                false,
                account_context,
                with_current,
            )
        };
    }
    let current = match unsafe { capture(account_context, with_current) } {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    let expected = ExpectedAccount::from_view(&current);
    let change = match parse_account_change(&user_info) {
        Ok(change) => change,
        Err(error) => {
            let failure = diagnosed(error);
            report_failure(account_context, with_current, &expected, &failure, false);
            return lifecycle(failure);
        }
    };
    let (token, profile) = match change {
        AccountChange::Login { token, profile } => (token, profile),
        AccountChange::ConfirmLogin { token, user_id } => {
            if current.token == token && current.user_id == user_id {
                return lifecycle(success_empty());
            }
            return lifecycle(stable_failure("stale_account_response", 409));
        }
        AccountChange::ConfirmCurrent(profile) => {
            if current.user_id == profile.user_id
                && current.user_name == profile.user_name
                && current.avatar == profile.avatar
            {
                return lifecycle(success_empty());
            }
            return lifecycle(stable_failure("stale_account_response", 409));
        }
    };
    let candidate = match Candidate::new(token, profile) {
        Ok(candidate) => candidate,
        Err(error) => {
            let failure = diagnosed(error);
            report_failure(account_context, with_current, &expected, &failure, false);
            return lifecycle(failure);
        }
    };
    finish_commit(commit_login(
        account_context,
        with_current,
        &expected,
        &candidate,
    ))
}

#[unsafe(no_mangle)]
/// # Safety
/// `ticket_ptr` must be valid for `ticket_len`; `account_context` plus `with_current` must remain
/// valid for every callback made during this synchronous call.
pub unsafe extern "C" fn pandar_plugin_account_exchange_ticket(
    ticket_ptr: *const u8,
    ticket_len: usize,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let current = match unsafe { capture(account_context, with_current) } {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    let expected = ExpectedAccount::from_view(&current);
    let Some(ticket) =
        unsafe { read_utf8(ticket_ptr, ticket_len) }.filter(|ticket| !ticket.trim().is_empty())
    else {
        let failure = stable_failure("invalid_plugin_ticket", 401);
        report_failure(account_context, with_current, &expected, &failure, true);
        return lifecycle(failure);
    };
    match fence_firmware(account_context, with_current, &expected) {
        CommitState::Applied => {}
        CommitState::Stale => return lifecycle(stable_failure("stale_account_response", 409)),
        CommitState::Failed(failure) => return lifecycle(failure),
        CommitState::Pending => return lifecycle(stable_failure("account_state_unavailable", 0)),
    }
    let pending = super::retry_pending_revocation(&expected.config_dir);
    if pending.status != 0 {
        report_failure(account_context, with_current, &expected, &pending, true);
        return lifecycle(pending);
    }
    let response = take_http(unsafe {
        pandar_plugin_exchange_ticket(
            expected.hub_url.as_ptr(),
            expected.hub_url.len(),
            ticket.as_ptr(),
            ticket.len(),
        )
    });
    if response.status != 0 {
        report_failure(account_context, with_current, &expected, &response, true);
        return lifecycle(response);
    }
    let candidate = match Candidate::from_session(&response.body) {
        Ok(candidate) => candidate,
        Err(error) => {
            let failure = diagnosed(error);
            report_failure(account_context, with_current, &expected, &failure, false);
            return lifecycle(failure);
        }
    };
    let state = commit_login(account_context, with_current, &expected, &candidate);
    if !matches!(state, CommitState::Applied) {
        revoke_ticket_candidate(&expected, &candidate);
    }
    match state {
        CommitState::Applied => lifecycle(success_token(&candidate.token)),
        CommitState::Stale => lifecycle(stable_failure("stale_account_response", 409)),
        CommitState::Failed(failure) => lifecycle(failure),
        CommitState::Pending => lifecycle(stable_failure("account_state_unavailable", 0)),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `token_ptr` must be valid for `token_len`; `account_context` plus `with_current` must remain
/// valid for every callback made during this synchronous call.
pub unsafe extern "C" fn pandar_plugin_account_profile(
    token_ptr: *const u8,
    token_len: usize,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) -> PluginLifecycleResult {
    let Some(requested_token) = (unsafe { read_utf8(token_ptr, token_len) }) else {
        return lifecycle(stable_failure("account_state_unavailable", 0));
    };
    let current = match unsafe { capture(account_context, with_current) } {
        Ok(current) => current,
        Err(error) => return lifecycle(diagnosed(error)),
    };
    let expected = ExpectedAccount::from_view(&current);
    if !requested_token.is_empty() && requested_token != current.token {
        return lifecycle(stable_failure("stale_account_response", 409));
    }
    if current.user_id.is_empty() || current.user_name.is_empty() || current.token.is_empty() {
        let failure = stable_failure("profile_unavailable", 401);
        report_failure(account_context, with_current, &expected, &failure, true);
        return lifecycle(failure);
    }
    lifecycle(success_profile(
        &current.user_id,
        &current.user_name,
        &current.avatar,
    ))
}

fn finish_commit(state: CommitState) -> PluginLifecycleResult {
    match state {
        CommitState::Applied => lifecycle(success_empty()),
        CommitState::Stale => lifecycle(stable_failure("stale_account_response", 409)),
        CommitState::Failed(failure) => lifecycle(failure),
        CommitState::Pending => lifecycle(stable_failure("account_state_unavailable", 0)),
    }
}

fn success_empty() -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 0,
        http_code: 200,
        body: String::new(),
    }
}

fn success_token(token: &str) -> NoAuthRotationOutcome {
    json_outcome(serde_json::to_string(&StudioToken {
        access_token: token,
        refresh_token: "",
        expires_in: 31_536_000,
        refresh_expires_in: 31_536_000,
        tfa_key: "",
        access_method: "pandar",
        login_type: "pandar",
    }))
}

fn success_profile(user_id: &str, user_name: &str, avatar: &str) -> NoAuthRotationOutcome {
    json_outcome(serde_json::to_string(&StudioProfile {
        user_id,
        account: user_name,
        name: user_name,
        avatar,
    }))
}

fn json_outcome(body: serde_json::Result<String>) -> NoAuthRotationOutcome {
    match body {
        Ok(body) => NoAuthRotationOutcome {
            status: 0,
            http_code: 200,
            body,
        },
        Err(error) => diagnosed(error.into()),
    }
}

fn stable_failure(error: &str, http_code: u32) -> NoAuthRotationOutcome {
    NoAuthRotationOutcome {
        status: 1,
        http_code,
        body: stable_error_body(error),
    }
}

fn diagnosed(error: anyhow::Error) -> NoAuthRotationOutcome {
    eprintln!("pandar authenticated account lifecycle failed: {error:#}");
    stable_failure("account_state_unavailable", 0)
}

fn lifecycle(outcome: NoAuthRotationOutcome) -> PluginLifecycleResult {
    PluginLifecycleResult {
        http: into_http(outcome),
        account_event: ACCOUNT_EVENT_NONE,
        report_http_error: 0,
    }
}
