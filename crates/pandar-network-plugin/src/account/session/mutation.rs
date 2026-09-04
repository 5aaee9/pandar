use std::ffi::c_void;

use anyhow::{Context, ensure};

use super::{
    AccountCallback, AccountLifecycleSession, ExpectedAccount, PluginAccountSessionBridge,
    TransitionCallback,
};
use crate::{
    account::{
        lifecycle::transaction::{
            AccountView, PluginAccountBytes, PluginAccountMutation, PluginAccountNotification,
        },
        types::Profile,
    },
    connection::{
        ffi::session as connection_session, pandar_plugin_printer_refresh_session_set_tenant,
        pandar_plugin_printer_refresh_session_update,
        pandar_plugin_studio_begin_account_transition,
    },
    firmware::session_ref as firmware_session,
};

pub(super) use crate::account::lifecycle::transaction::{
    MUTATION_CLEAR, MUTATION_FIRMWARE_FENCE, MUTATION_HTTP_ERROR, MUTATION_LOGIN, MUTATION_REPLACE,
    MUTATION_RESTORE_FAILURE, MUTATION_RUNTIME_HUB, MUTATION_RUNTIME_SERVERS,
};

pub(super) unsafe fn apply_mutation(
    session: &AccountLifecycleSession,
    connection_ptr: *mut c_void,
    firmware_ptr: *mut c_void,
    bridge: &PluginAccountSessionBridge,
    agent: *mut c_void,
    current: &AccountView,
    mutation: &PluginAccountMutation,
) -> anyhow::Result<()> {
    unsafe {
        match mutation.action {
            0 => return Ok(()),
            MUTATION_REPLACE => {
                let account = read_replacement(mutation)?;
                replace(bridge, agent, &account);
                sync_sessions(connection_ptr, firmware_ptr, &current.hub_url, &account)?;
            }
            MUTATION_LOGIN => {
                let account = read_replacement(mutation)?;
                let epoch = begin_transition(
                    connection_ptr,
                    firmware_ptr,
                    &current.hub_url,
                    &current.token,
                )?;
                replace(bridge, agent, &account);
                sync_sessions(connection_ptr, firmware_ptr, &current.hub_url, &account)?;
                session.enqueue(AccountCallback::Transition(TransitionCallback {
                    account_epoch: epoch,
                    notification: Some(true),
                    expected: None,
                    error: None,
                }));
            }
            MUTATION_CLEAR => {
                let epoch = begin_transition(
                    connection_ptr,
                    firmware_ptr,
                    &current.hub_url,
                    &current.token,
                )?;
                (bridge.clear)(agent);
                sync_empty_sessions(connection_ptr, firmware_ptr, &current.hub_url)?;
                session.enqueue(AccountCallback::Transition(TransitionCallback {
                    account_epoch: epoch,
                    notification: (mutation.notification == PluginAccountNotification::Logout)
                        .then_some(false),
                    expected: Some(ExpectedAccount {
                        hub_url: current.hub_url.clone(),
                        token: String::new(),
                        account_epoch: epoch,
                        config_epoch: current.config_epoch,
                        session_kind: 0,
                    }),
                    error: None,
                }));
            }
            MUTATION_HTTP_ERROR => session.enqueue(AccountCallback::HttpError(
                mutation.http_code,
                mutation.error_body.read("account HTTP error body")?,
            )),
            MUTATION_RUNTIME_HUB => {
                let hub_url = mutation.hub_url.read("runtime Hub URL")?;
                let epoch = begin_transition(
                    connection_ptr,
                    firmware_ptr,
                    &current.hub_url,
                    &current.token,
                )?;
                (bridge.clear)(agent);
                (bridge.set_hub_url)(agent, PluginAccountBytes::from_str(&hub_url));
                sync_empty_sessions(connection_ptr, firmware_ptr, &hub_url)?;
                session.enqueue(AccountCallback::Transition(TransitionCallback {
                    account_epoch: epoch,
                    notification: None,
                    expected: None,
                    error: None,
                }));
            }
            MUTATION_RUNTIME_SERVERS => {
                let hub_url = mutation.hub_url.read("runtime Hub URL")?;
                let frontend_url = mutation.frontend_url.read("runtime frontend URL")?;
                let epoch = begin_transition(
                    connection_ptr,
                    firmware_ptr,
                    &current.hub_url,
                    &current.token,
                )?;
                (bridge.clear)(agent);
                (bridge.set_hub_url)(agent, PluginAccountBytes::from_str(&hub_url));
                (bridge.set_frontend_url)(agent, PluginAccountBytes::from_str(&frontend_url));
                sync_empty_sessions(connection_ptr, firmware_ptr, &hub_url)?;
                session.enqueue(AccountCallback::Transition(TransitionCallback {
                    account_epoch: epoch,
                    notification: None,
                    expected: None,
                    error: None,
                }));
            }
            MUTATION_FIRMWARE_FENCE => {
                let firmware =
                    firmware_session(firmware_ptr).context("firmware session is missing")?;
                firmware.fence_account(current.hub_url.clone(), current.token.clone());
                return Ok(());
            }
            MUTATION_RESTORE_FAILURE => {
                let account = read_replacement(mutation)?;
                let epoch = begin_transition(
                    connection_ptr,
                    firmware_ptr,
                    &current.hub_url,
                    &current.token,
                )?;
                replace(bridge, agent, &account);
                sync_sessions(connection_ptr, firmware_ptr, &current.hub_url, &account)?;
                session.enqueue(AccountCallback::Transition(TransitionCallback {
                    account_epoch: epoch,
                    notification: Some(true),
                    expected: Some(ExpectedAccount {
                        hub_url: current.hub_url.clone(),
                        token: account.token.clone(),
                        account_epoch: epoch,
                        config_epoch: current.config_epoch,
                        session_kind: account.session_kind,
                    }),
                    error: Some((
                        mutation.http_code,
                        mutation.error_body.read("account restore error body")?,
                    )),
                }));
            }
            action => anyhow::bail!("unknown account mutation action {action}"),
        }
        if mutation.action != MUTATION_HTTP_ERROR && mutation.action != MUTATION_FIRMWARE_FENCE {
            (bridge.reset_personal_presets)(agent);
        }
        Ok(())
    }
}

struct Replacement {
    token: String,
    user_id: String,
    user_name: String,
    avatar: String,
    profile_json: String,
    tenant_id: String,
    session_kind: i32,
}

unsafe fn read_replacement(mutation: &PluginAccountMutation) -> anyhow::Result<Replacement> {
    let profile_json = unsafe { mutation.profile_json.read("account profile") }?;
    let profile: Profile = serde_json::from_str(&profile_json).context("decode account profile")?;
    Ok(Replacement {
        token: unsafe { mutation.token.read("account token") }?,
        user_id: unsafe { mutation.user_id.read("account user id") }?,
        user_name: unsafe { mutation.user_name.read("account user name") }?,
        avatar: unsafe { mutation.avatar.read("account avatar") }?,
        profile_json,
        tenant_id: profile.tenant_id,
        session_kind: mutation.session_kind,
    })
}

fn replace(bridge: &PluginAccountSessionBridge, agent: *mut c_void, account: &Replacement) {
    (bridge.replace)(
        agent,
        PluginAccountBytes::from_str(&account.token),
        PluginAccountBytes::from_str(&account.user_id),
        PluginAccountBytes::from_str(&account.user_name),
        PluginAccountBytes::from_str(&account.avatar),
        PluginAccountBytes::from_str(&account.profile_json),
        PluginAccountBytes::from_str(&account.tenant_id),
        account.session_kind,
    );
}

unsafe fn sync_sessions(
    connection_ptr: *mut c_void,
    firmware_ptr: *mut c_void,
    hub_url: &str,
    account: &Replacement,
) -> anyhow::Result<()> {
    ensure!(
        unsafe {
            pandar_plugin_printer_refresh_session_update(
                connection_ptr,
                hub_url.as_ptr(),
                hub_url.len(),
                account.token.as_ptr(),
                account.token.len(),
            )
        } == 0,
        "update printer refresh account"
    );
    ensure!(
        unsafe {
            pandar_plugin_printer_refresh_session_set_tenant(
                connection_ptr,
                account.tenant_id.as_ptr(),
                account.tenant_id.len(),
            )
        } == 0,
        "update printer refresh tenant"
    );
    let firmware =
        unsafe { firmware_session(firmware_ptr) }.context("firmware session is missing")?;
    firmware.sync_account(hub_url.to_owned(), account.token.clone());
    Ok(())
}

unsafe fn sync_empty_sessions(
    connection_ptr: *mut c_void,
    firmware_ptr: *mut c_void,
    hub_url: &str,
) -> anyhow::Result<()> {
    let account = Replacement {
        token: String::new(),
        user_id: String::new(),
        user_name: String::new(),
        avatar: String::new(),
        profile_json: String::new(),
        tenant_id: String::new(),
        session_kind: 0,
    };
    unsafe { sync_sessions(connection_ptr, firmware_ptr, hub_url, &account) }
}

unsafe fn begin_transition(
    connection_ptr: *mut c_void,
    firmware_ptr: *mut c_void,
    hub_url: &str,
    token: &str,
) -> anyhow::Result<u64> {
    let connection = unsafe { connection_session(connection_ptr) }
        .context("printer refresh session is missing")?;
    let (state, _) = connection.studio_request_snapshot(String::new());
    ensure!(
        unsafe { pandar_plugin_studio_begin_account_transition(connection_ptr) } == 0,
        "begin account printer transition"
    );
    let firmware =
        unsafe { firmware_session(firmware_ptr) }.context("firmware session is missing")?;
    firmware.fence_account(hub_url.to_owned(), token.to_owned());
    Ok(state.account_epoch.wrapping_add(1))
}
