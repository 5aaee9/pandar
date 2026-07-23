use std::ffi::c_void;

use anyhow::Context;

use crate::{
    account::{
        persistence, revocation,
        types::{PendingRevocation, PersistedLogin, SessionKind},
    },
    connection::no_auth_rotation::NoAuthRotationOutcome,
};

use super::{Candidate, ExpectedAccount, diagnosed};
use crate::account::lifecycle::transaction::{
    AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountView,
    PluginWithCurrentAccount, transact,
};

const MUTATION_HTTP_ERROR: i32 = 3;
const MUTATION_LOGIN: i32 = 4;
const MUTATION_FIRMWARE_FENCE: i32 = 7;

pub(super) enum CommitState {
    Pending,
    Applied,
    Stale,
    Failed(NoAuthRotationOutcome),
}

struct CommitContext<'a> {
    expected: &'a ExpectedAccount,
    candidate: &'a Candidate,
    state: CommitState,
}

struct ErrorContext<'a> {
    expected: &'a ExpectedAccount,
    failure: &'a NoAuthRotationOutcome,
}

struct FirmwareFenceContext<'a> {
    expected: &'a ExpectedAccount,
    state: CommitState,
}

pub(super) fn commit_login(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &ExpectedAccount,
    candidate: &Candidate,
) -> CommitState {
    let mut context = CommitContext {
        expected,
        candidate,
        state: CommitState::Pending,
    };
    if let Err(error) = transact(
        account_context,
        with_current,
        (&mut context as *mut CommitContext<'_>).cast(),
        login_transaction,
    ) {
        return CommitState::Failed(diagnosed(error));
    }
    context.state
}

pub(super) fn fence_firmware(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &ExpectedAccount,
) -> CommitState {
    let mut context = FirmwareFenceContext {
        expected,
        state: CommitState::Pending,
    };
    if let Err(error) = transact(
        account_context,
        with_current,
        (&mut context as *mut FirmwareFenceContext<'_>).cast(),
        firmware_fence_transaction,
    ) {
        return CommitState::Failed(diagnosed(error));
    }
    context.state
}

pub(super) fn revoke_ticket_candidate(expected: &ExpectedAccount, candidate: &Candidate) {
    let pending = PendingRevocation {
        hub_url: expected.hub_url.clone(),
        token: candidate.token.clone(),
    };
    let staged = match persistence::enqueue_pending(&expected.config_dir, pending.clone()) {
        Ok(persistence::MutationDurability::Confirmed) => true,
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            eprintln!(
                "pandar ticket candidate staging failed: change published but durability was not confirmed: {error:#}"
            );
            false
        }
        Err(error) => {
            eprintln!("pandar ticket candidate staging failed: {error:#}");
            false
        }
    };
    let response = if staged {
        match revocation::revoke(&expected.config_dir, pending) {
            Ok(Some(response)) => Some(crate::account::lifecycle::take_http(response)),
            Ok(None) => None,
            Err(error) => {
                eprintln!("pandar ticket candidate revoke failed: {error:#}");
                None
            }
        }
    } else {
        match revocation::revoke_orphan(&expected.config_dir, pending) {
            Ok(Some(response)) => Some(crate::account::lifecycle::take_http(response)),
            Ok(None) => None,
            Err(error) => {
                eprintln!("pandar ticket candidate direct revoke failed: {error:#}");
                None
            }
        }
    };
    if let Some(response) = response.filter(|response| response.status != 0) {
        eprintln!(
            "pandar ticket candidate revoke failed: status={} http_code={} body={}",
            response.status, response.http_code, response.body
        );
    }
}

pub(super) fn report_failure(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &ExpectedAccount,
    failure: &NoAuthRotationOutcome,
    callback: bool,
) {
    if !callback {
        return;
    }
    let mut context = ErrorContext { expected, failure };
    if let Err(error) = transact(
        account_context,
        with_current,
        (&mut context as *mut ErrorContext<'_>).cast(),
        error_transaction,
    ) {
        eprintln!("pandar authenticated account failure delivery failed: {error:#}");
    }
}

unsafe extern "C" fn login_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<CommitContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        if !context.expected.matches(&current) {
            context.state = CommitState::Stale;
            return Ok(());
        }
        let login = PersistedLogin {
            hub_url: current.hub_url,
            token: context.candidate.token.clone(),
            session_kind: SessionKind::Authenticated,
            profile: context.candidate.profile.clone(),
        };
        match persistence::store(&current.config_dir, &login) {
            Ok(durability) => {
                if let Err(error) =
                    durability.require_confirmed("durably persist authenticated Studio login")
                {
                    context.state = CommitState::Failed(diagnosed(error));
                    return Ok(());
                }
            }
            Err(error) => {
                let failure = diagnosed(error.context("persist authenticated Studio login"));
                context.state = CommitState::Failed(failure);
                return Ok(());
            }
        }
        set_candidate(mutation, MUTATION_LOGIN, context.candidate)?;
        context.state = CommitState::Applied;
        Ok(())
    })();
    transaction_status(work, &mut context.state)
}

unsafe extern "C" fn firmware_fence_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<FirmwareFenceContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        if !context.expected.matches(&current) {
            context.state = CommitState::Stale;
            return Ok(());
        }
        let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
        mutation.action = MUTATION_FIRMWARE_FENCE;
        context.state = CommitState::Applied;
        Ok(())
    })();
    transaction_status(work, &mut context.state)
}

unsafe extern "C" fn error_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<ErrorContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        if context.expected.matches(&current) {
            set_error(mutation, MUTATION_HTTP_ERROR, context.failure)?;
        }
        Ok(())
    })();
    work.map_or(1, |()| 0)
}

fn set_candidate(
    mutation: *mut PluginAccountMutation,
    action: i32,
    candidate: &Candidate,
) -> anyhow::Result<()> {
    let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
    mutation.action = action;
    mutation.token = PluginAccountBytes::from_str(&candidate.token);
    mutation.user_id = PluginAccountBytes::from_str(&candidate.profile.user_id);
    mutation.user_name = PluginAccountBytes::from_str(&candidate.profile.user_name);
    mutation.avatar = PluginAccountBytes::from_str(&candidate.profile.avatar);
    mutation.profile_json = PluginAccountBytes::from_str(&candidate.profile_json);
    mutation.session_kind = SessionKind::Authenticated as i32;
    Ok(())
}

fn set_error(
    mutation: *mut PluginAccountMutation,
    action: i32,
    failure: &NoAuthRotationOutcome,
) -> anyhow::Result<()> {
    let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
    mutation.action = action;
    mutation.error_body = PluginAccountBytes::from_str(&failure.body);
    mutation.http_code = failure.http_code;
    Ok(())
}

fn transaction_status(work: anyhow::Result<()>, state: &mut CommitState) -> i32 {
    match work {
        Ok(()) => 0,
        Err(error) => {
            *state = CommitState::Failed(diagnosed(error));
            1
        }
    }
}
