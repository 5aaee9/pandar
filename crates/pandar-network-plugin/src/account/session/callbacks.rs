use std::ffi::c_void;

use super::{
    AccountCallback, AccountLifecycleSession, ExpectedAccount, PluginAccountSessionBridge,
};
use crate::{
    account::lifecycle::transaction::{PluginAccountBytes, PluginWithCurrentAccount, capture},
    connection::{
        ffi::session as connection_session, pandar_plugin_studio_finish_account_transition,
    },
    dispatch::{PluginDispatchBridge, dispatch_transition_and_tickets},
};

#[unsafe(no_mangle)]
/// # Safety
/// All pointers and bridges must remain live for the call.
pub unsafe extern "C" fn pandar_plugin_account_session_drain(
    session_ptr: *mut c_void,
    connection_ptr: *mut c_void,
    dispatch_bridge_ptr: *const PluginDispatchBridge,
    account_bridge_ptr: *const PluginAccountSessionBridge,
    agent: *mut c_void,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) {
    let (Some(session), Some(connection), Some(account_bridge)) = (
        unsafe { session_ptr.cast::<AccountLifecycleSession>().as_ref() },
        connection_session(connection_ptr),
        unsafe { account_bridge_ptr.as_ref() },
    ) else {
        return;
    };
    {
        let mut callbacks = session
            .callbacks
            .lock()
            .expect("account callback queue poisoned");
        if callbacks.draining {
            return;
        }
        callbacks.draining = true;
    }
    loop {
        let callback = session
            .callbacks
            .lock()
            .expect("account callback queue poisoned")
            .pending
            .pop_front();
        let Some(callback) = callback else {
            session
                .callbacks
                .lock()
                .expect("account callback queue poisoned")
                .draining = false;
            return;
        };
        match callback {
            AccountCallback::UserLogin(login) => {
                (account_bridge.invoke_user_login)(agent, i32::from(login), login);
            }
            AccountCallback::HttpError(code, body) => {
                invoke_http_error(account_bridge, agent, code, &body);
            }
            AccountCallback::Transition(callback) => {
                let Some(dispatch_bridge) = (unsafe { dispatch_bridge_ptr.as_ref() }) else {
                    continue;
                };
                let transition = connection.take_transition();
                let tickets: Vec<u64> = connection
                    .take_offline()
                    .into_iter()
                    .map(|delivery| delivery.ticket)
                    .collect();
                dispatch_transition_and_tickets(
                    dispatch_bridge,
                    agent,
                    connection_ptr,
                    connection,
                    transition,
                    &tickets,
                );
                pandar_plugin_studio_finish_account_transition(
                    connection_ptr,
                    callback.account_epoch,
                );
                let current = callback
                    .expected
                    .as_ref()
                    .is_none_or(|expected| expected.matches(account_context, with_current));
                if current && let Some(login) = callback.notification {
                    (account_bridge.invoke_user_login)(agent, i32::from(login), login);
                }
                if current
                    && let Some((code, body)) = callback.error
                    && !body.is_empty()
                    && callback
                        .expected
                        .as_ref()
                        .is_none_or(|expected| expected.matches(account_context, with_current))
                {
                    invoke_http_error(account_bridge, agent, code, &body);
                }
            }
        }
    }
}

impl ExpectedAccount {
    fn matches(
        &self,
        account_context: *mut c_void,
        with_current: Option<PluginWithCurrentAccount>,
    ) -> bool {
        capture(account_context, with_current).is_ok_and(|current| self.matches_view(&current))
    }

    pub(super) fn matches_view(
        &self,
        current: &crate::account::lifecycle::transaction::AccountView,
    ) -> bool {
        !current.transition_pending
            && current.hub_url == self.hub_url
            && current.token == self.token
            && current.account_epoch == self.account_epoch
            && current.config_epoch == self.config_epoch
            && current.session_kind == self.session_kind
    }
}

fn invoke_http_error(
    bridge: &PluginAccountSessionBridge,
    agent: *mut c_void,
    code: u32,
    body: &str,
) {
    (bridge.invoke_http_error)(agent, code, PluginAccountBytes::from_str(body));
}
