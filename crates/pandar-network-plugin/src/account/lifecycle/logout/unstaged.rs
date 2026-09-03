use std::ffi::c_void;

use anyhow::Context;

use super::{
    LoggedOutExpected, diagnosed_outcome, report_remote_failure, revoke_unstaged, success_outcome,
};
use crate::{
    account::{
        lifecycle::{
            authenticated::ExpectedAccount,
            transaction::{
                AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountNotification,
                PluginAccountView, PluginWithCurrentAccount, transact,
            },
        },
        persistence,
        types::PendingRevocation,
    },
    connection::no_auth_rotation::NoAuthRotationOutcome,
};

const MUTATION_CLEAR: i32 = 2;
const MUTATION_HTTP_ERROR: i32 = 3;

#[derive(Clone, Copy)]
pub(super) enum RevocationStage {
    Failed,
    Staged,
}

pub(super) struct UnstagedLogout {
    expected: ExpectedAccount,
    candidate: PendingRevocation,
}

enum ClearState {
    Pending,
    Applied,
    Stale,
    Failed(NoAuthRotationOutcome),
}

struct ClearContext<'a> {
    expected: &'a ExpectedAccount,
    state: ClearState,
}

struct ReportContext<'a> {
    expected: &'a ExpectedAccount,
    failure: &'a NoAuthRotationOutcome,
}

impl UnstagedLogout {
    pub(super) fn new(current: &AccountView, candidate: PendingRevocation) -> Self {
        Self {
            expected: ExpectedAccount::from_view(current),
            candidate,
        }
    }

    fn logged_out_expected(&self) -> LoggedOutExpected {
        LoggedOutExpected {
            hub_url: self.expected.hub_url.clone(),
            account_epoch: self.expected.account_epoch.wrapping_add(1),
            config_epoch: self.expected.config_epoch,
        }
    }
}

pub(super) fn stage_revocation(config_dir: &str, candidate: &PendingRevocation) -> RevocationStage {
    if config_dir.is_empty() {
        eprintln!(
            "pandar account logout lifecycle failed: stage pending plugin session revocation: Studio config directory is empty"
        );
        return RevocationStage::Failed;
    }
    match persistence::enqueue_pending(config_dir, candidate.clone()) {
        Ok(persistence::MutationDurability::Confirmed) => RevocationStage::Staged,
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            eprintln!(
                "pandar account logout lifecycle failed: pending plugin session revocation change published but durability was not confirmed: {error:#}"
            );
            RevocationStage::Failed
        }
        Err(error) => {
            let error = error.context("stage pending plugin session revocation");
            eprintln!("pandar account logout lifecycle failed: {error:#}");
            RevocationStage::Failed
        }
    }
}

pub(super) fn finish_unstaged(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    work: UnstagedLogout,
) -> NoAuthRotationOutcome {
    match persistence::prepare_direct(&work.expected.config_dir, &work.candidate) {
        Ok(persistence::MutationDurability::Confirmed) => {}
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            let failure = diagnosed_outcome(
                error.context("direct plugin revocation intent durability is unconfirmed"),
            );
            let cleared = clear_current(account_context, with_current, &work.expected);
            if cleared.status != 0 {
                return cleared;
            }
            report_remote_failure(
                account_context,
                with_current,
                &work.logged_out_expected(),
                &failure,
            );
            return failure;
        }
        Err(error) => {
            let failure = diagnosed_outcome(error.context("prepare direct plugin revocation"));
            report_failure(account_context, with_current, &work.expected, &failure);
            return failure;
        }
    }
    let remote = revoke_unstaged(work.candidate.clone());
    if remote.status != 0 {
        let cleared = clear_current(account_context, with_current, &work.expected);
        if cleared.status != 0 {
            return cleared;
        }
        report_remote_failure(
            account_context,
            with_current,
            &work.logged_out_expected(),
            &remote,
        );
        return remote;
    }
    let cleared = clear_current(account_context, with_current, &work.expected);
    if cleared.status != 0 {
        return cleared;
    }
    match persistence::complete_direct(&work.expected.config_dir, &work.candidate) {
        Ok(()) => success_outcome(),
        Err(error) => {
            let failure = diagnosed_outcome(error.context("complete direct plugin revocation"));
            report_remote_failure(
                account_context,
                with_current,
                &work.logged_out_expected(),
                &failure,
            );
            failure
        }
    }
}

fn clear_current(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &ExpectedAccount,
) -> NoAuthRotationOutcome {
    let mut context = ClearContext {
        expected,
        state: ClearState::Pending,
    };
    if let Err(error) = unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut ClearContext<'_>).cast(),
            clear_transaction,
        )
    } {
        return diagnosed_outcome(error);
    }
    match context.state {
        ClearState::Applied | ClearState::Stale => success_outcome(),
        ClearState::Failed(failure) => failure,
        ClearState::Pending => diagnosed_outcome(anyhow::anyhow!(
            "account transaction did not finish unstaged logout"
        )),
    }
}

unsafe extern "C" fn clear_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<ClearContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = unsafe { AccountView::read(view) }?;
        if !context.expected.matches(&current) {
            context.state = ClearState::Stale;
            return Ok(());
        }
        let mutation =
            unsafe { mutation.as_mut() }.context("account logout mutation is missing")?;
        mutation.action = MUTATION_CLEAR;
        mutation.notification = PluginAccountNotification::Logout;
        context.state = ClearState::Applied;
        Ok(())
    })();
    transaction_status(work, &mut context.state)
}

fn report_failure(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    expected: &ExpectedAccount,
    failure: &NoAuthRotationOutcome,
) {
    let mut context = ReportContext { expected, failure };
    if let Err(error) = unsafe {
        transact(
            account_context,
            with_current,
            (&mut context as *mut ReportContext<'_>).cast(),
            report_transaction,
        )
    } {
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
        let current = unsafe { AccountView::read(view) }?;
        if context.expected.matches(&current) {
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

fn transaction_status(work: anyhow::Result<()>, state: &mut ClearState) -> i32 {
    match work {
        Ok(()) => 0,
        Err(error) => {
            *state = ClearState::Failed(diagnosed_outcome(error));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountView {
        AccountView {
            config_dir: "config".to_owned(),
            hub_url: "http://127.0.0.1:18080".to_owned(),
            frontend_url: "http://localhost:13000".to_owned(),
            token: "token".to_owned(),
            user_id: "user".to_owned(),
            user_name: "User".to_owned(),
            avatar: String::new(),
            profile_json: String::new(),
            account_epoch: 7,
            config_epoch: 11,
            session_kind: 2,
            transition_pending: false,
        }
    }

    #[test]
    fn retained_logout_fence_rejects_each_account_identity_change() {
        let current = account();
        let expected = ExpectedAccount::from_view(&current);
        assert!(expected.matches(&current));

        let mut variants = Vec::new();
        let mut changed = current.clone();
        changed.config_dir = "replacement-config".to_owned();
        variants.push(changed);
        let mut changed = current.clone();
        changed.hub_url = "http://127.0.0.1:28080".to_owned();
        variants.push(changed);
        let mut changed = current.clone();
        changed.token = "replacement-token".to_owned();
        variants.push(changed);
        let mut changed = current.clone();
        changed.account_epoch += 1;
        variants.push(changed);
        let mut changed = current.clone();
        changed.config_epoch += 1;
        variants.push(changed);
        let mut changed = current.clone();
        changed.session_kind = 1;
        variants.push(changed);
        let mut changed = current;
        changed.transition_pending = true;
        variants.push(changed);

        assert!(variants.iter().all(|changed| !expected.matches(changed)));
    }
}
