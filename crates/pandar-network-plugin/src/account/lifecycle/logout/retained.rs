use std::ffi::c_void;

use anyhow::Context;

use super::{
    LoggedOutExpected, diagnosed_outcome, report_remote_failure, revoke_staged, revoke_unstaged,
    success_outcome,
    unstaged::{RevocationStage, stage_revocation},
};
use crate::{
    account::{
        lifecycle::{
            authenticated::ExpectedAccount,
            transaction::{
                AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountView,
                PluginWithCurrentAccount, transact,
            },
        },
        persistence,
        types::PendingRevocation,
    },
    connection::no_auth_rotation::NoAuthRotationOutcome,
};

const MUTATION_RESTORE_FAILURE: i32 = 8;

pub(super) struct RetainedLogout {
    expected: ExpectedAccount,
    snapshot: AccountSnapshot,
    candidate: PendingRevocation,
}

struct AccountSnapshot {
    token: String,
    user_id: String,
    user_name: String,
    avatar: String,
    profile_json: String,
    session_kind: i32,
}

enum RestoreState {
    Pending,
    Applied,
    Stale,
    Failed,
}

struct RestoreContext<'a> {
    expected: &'a ExpectedAccount,
    snapshot: &'a AccountSnapshot,
    failure: Option<&'a NoAuthRotationOutcome>,
    state: RestoreState,
}

impl RetainedLogout {
    pub(super) fn new(current: &AccountView) -> Self {
        Self {
            expected: ExpectedAccount {
                config_dir: current.config_dir.clone(),
                hub_url: current.hub_url.clone(),
                token: String::new(),
                account_epoch: current.account_epoch.wrapping_add(1),
                config_epoch: current.config_epoch,
                session_kind: 0,
            },
            snapshot: AccountSnapshot {
                token: current.token.clone(),
                user_id: current.user_id.clone(),
                user_name: current.user_name.clone(),
                avatar: current.avatar.clone(),
                profile_json: current.profile_json.clone(),
                session_kind: current.session_kind,
            },
            candidate: PendingRevocation {
                hub_url: current.hub_url.clone(),
                token: current.token.clone(),
            },
        }
    }

    fn logged_out_expected(&self) -> LoggedOutExpected {
        LoggedOutExpected {
            hub_url: self.expected.hub_url.clone(),
            account_epoch: self.expected.account_epoch,
            config_epoch: self.expected.config_epoch,
        }
    }
}

pub(super) fn finish_retained(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    work: RetainedLogout,
    request: bool,
) -> NoAuthRotationOutcome {
    if !request {
        return match persistence::clear_matching(&work.expected.config_dir, &work.candidate) {
            Ok(persistence::MutationDurability::Confirmed) => success_outcome(),
            Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
                let failure = diagnosed_outcome(
                    error.context("passive Studio login removal durability is unconfirmed"),
                );
                report_remote_failure(
                    account_context,
                    with_current,
                    &work.logged_out_expected(),
                    &failure,
                );
                failure
            }
            Err(error) => {
                let failure = diagnosed_outcome(error.context("clear passive Studio login"));
                restore(account_context, with_current, &work, None);
                failure
            }
        };
    }

    match stage_revocation(&work.expected.config_dir, &work.candidate) {
        RevocationStage::Staged => {
            let remote = revoke_staged(&work.expected.config_dir, work.candidate.clone());
            if remote.status != 0 {
                report_remote_failure(
                    account_context,
                    with_current,
                    &work.logged_out_expected(),
                    &remote,
                );
            }
            remote
        }
        RevocationStage::Failed => finish_unstaged(account_context, with_current, work),
    }
}

fn finish_unstaged(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    work: RetainedLogout,
) -> NoAuthRotationOutcome {
    match persistence::prepare_direct(&work.expected.config_dir, &work.candidate) {
        Ok(persistence::MutationDurability::Confirmed) => {}
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            let failure = diagnosed_outcome(
                error.context("direct plugin revocation intent durability is unconfirmed"),
            );
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
            restore(account_context, with_current, &work, Some(&failure));
            return failure;
        }
    }
    let remote = revoke_unstaged(work.candidate.clone());
    if remote.status != 0 {
        report_remote_failure(
            account_context,
            with_current,
            &work.logged_out_expected(),
            &remote,
        );
        return remote;
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

fn restore(
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    work: &RetainedLogout,
    failure: Option<&NoAuthRotationOutcome>,
) {
    let mut context = RestoreContext {
        expected: &work.expected,
        snapshot: &work.snapshot,
        failure,
        state: RestoreState::Pending,
    };
    let result = transact(
        account_context,
        with_current,
        (&mut context as *mut RestoreContext<'_>).cast(),
        restore_transaction,
    );
    if let Err(error) = result {
        eprintln!("pandar account logout restore failed: {error:#}");
    } else if matches!(context.state, RestoreState::Failed | RestoreState::Pending) {
        eprintln!("pandar account logout restore failed: account transaction did not restore");
    }
}

unsafe extern "C" fn restore_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<RestoreContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = AccountView::read(view)?;
        if !context.expected.matches(&current) {
            context.state = RestoreState::Stale;
            return Ok(());
        }
        let mutation =
            unsafe { mutation.as_mut() }.context("account restore mutation is missing")?;
        mutation.action = MUTATION_RESTORE_FAILURE;
        mutation.token = PluginAccountBytes::from_str(&context.snapshot.token);
        mutation.user_id = PluginAccountBytes::from_str(&context.snapshot.user_id);
        mutation.user_name = PluginAccountBytes::from_str(&context.snapshot.user_name);
        mutation.avatar = PluginAccountBytes::from_str(&context.snapshot.avatar);
        mutation.profile_json = PluginAccountBytes::from_str(&context.snapshot.profile_json);
        mutation.session_kind = context.snapshot.session_kind;
        if let Some(failure) = context.failure {
            mutation.error_body = PluginAccountBytes::from_str(&failure.body);
            mutation.http_code = failure.http_code;
        }
        context.state = RestoreState::Applied;
        Ok(())
    })();
    match work {
        Ok(()) => 0,
        Err(error) => {
            context.state = RestoreState::Failed;
            eprintln!("pandar account logout restore failed: {error:#}");
            1
        }
    }
}
