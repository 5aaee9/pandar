use std::ffi::c_void;

use anyhow::{Context, ensure};

use crate::connection::no_auth_rotation::NoAuthRotationOutcome;

use super::super::authenticated::ExpectedAccount;
use super::diagnosed;
use crate::account::lifecycle::transaction::{
    AccountView, MUTATION_REPLACE, MUTATION_RUNTIME_HUB, MUTATION_RUNTIME_SERVERS,
    PluginAccountBytes, PluginAccountMutation, PluginAccountView,
};
use crate::account::{persistence, types::PersistedLogin};

pub(super) struct LoadedAccount {
    token: String,
    user_id: String,
    user_name: String,
    avatar: String,
    profile_json: String,
    session_kind: i32,
}

pub(super) struct LoadContext<'a> {
    pub(super) expected: &'a ExpectedAccount,
    pub(super) loaded: &'a LoadedAccount,
    pub(super) state: ApplyState,
}

pub(super) struct RuntimeContext<'a> {
    pub(super) expected: &'a ExpectedAccount,
    pub(super) hub_url: &'a str,
    pub(super) state: ApplyState,
}

pub(super) struct RestoreServersContext<'a> {
    pub(super) expected: &'a ExpectedAccount,
    pub(super) hub_url: &'a str,
    pub(super) frontend_url: &'a str,
    pub(super) state: ApplyState,
}

pub(super) enum ApplyState {
    Pending,
    Applied,
    Stale,
    Failed(NoAuthRotationOutcome),
}

impl LoadedAccount {
    pub(super) fn from_login(login: PersistedLogin) -> anyhow::Result<Self> {
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

pub(super) unsafe extern "C" fn load_transaction(
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

pub(super) unsafe extern "C" fn runtime_transaction(
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

pub(super) unsafe extern "C" fn restore_servers_transaction(
    context: *mut c_void,
    view: *const PluginAccountView,
    mutation: *mut PluginAccountMutation,
) -> i32 {
    let Some(context) = (unsafe { context.cast::<RestoreServersContext<'_>>().as_mut() }) else {
        return 1;
    };
    let work: anyhow::Result<()> = (|| {
        let current = unsafe { AccountView::read(view) }?;
        if !context.expected.matches(&current) || !current.token.is_empty() {
            context.state = ApplyState::Stale;
            return Ok(());
        }
        let mutation = unsafe { mutation.as_mut() }.context("account mutation is missing")?;
        mutation.action = MUTATION_RUNTIME_SERVERS;
        mutation.hub_url = PluginAccountBytes::from_str(context.hub_url);
        mutation.frontend_url = PluginAccountBytes::from_str(context.frontend_url);
        context.state = ApplyState::Applied;
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
