use std::ffi::c_void;

use crate::{
    PluginHttpResult,
    account::lifecycle::{
        NoAuthExpected, NoAuthRecovery, PluginWithCurrentAccount, into_http, recover,
        recover_with_cancellation as recover_account_with_cancellation, take_http,
    },
};

use super::ffi::{
    PluginBytes, PluginStudioAccount, PluginStudioPlateResult, PluginStudioSnapshot,
    PluginStudioTaskQuery, StudioSnapshotCallback, pandar_plugin_studio_get_plate,
    pandar_plugin_studio_get_subtask, pandar_plugin_studio_get_tasks,
};
#[cfg(test)]
use super::model_task::pandar_plugin_studio_get_model_task;
use super::model_task::{
    ModelTaskCancellation, StudioModelTaskCancelled, StudioModelTaskVisitor, get_model_task,
};

#[derive(Clone, Copy)]
struct RecoveryContext {
    session: *mut c_void,
    config_epoch: u64,
    session_kind: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
}

/// # Safety
///
/// `account` and `query` must point to valid ABI values for the duration of this call. Any callback
/// stored in `account` or supplied through `with_current` must honor its declared pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_studio_get_tasks_with_session(
    session: *mut c_void,
    account: *const PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    query: *const PluginStudioTaskQuery,
) -> PluginHttpResult {
    let initial = unsafe { pandar_plugin_studio_get_tasks(account, query) };
    recover_http(
        initial,
        RecoveryContext {
            session,
            config_epoch,
            session_kind,
            account_context,
            with_current,
        },
        account,
        |retry| unsafe { pandar_plugin_studio_get_tasks(retry, query) },
    )
}

/// # Safety
///
/// `account` must point to a valid ABI value for the duration of this call. Any callback stored in
/// it or supplied through `with_current` must honor its declared pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_studio_get_plate_with_session(
    session: *mut c_void,
    account: *const PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    task_id: PluginBytes,
) -> PluginStudioPlateResult {
    let initial = pandar_plugin_studio_get_plate(account, task_id);
    if !auth_rejected(&initial.http) {
        return initial;
    }
    match recovery(
        RecoveryContext {
            session,
            config_epoch,
            session_kind,
            account_context,
            with_current,
        },
        account,
    ) {
        Recovery::Original => initial,
        Recovery::Failure(http) => {
            take_http(initial.http);
            PluginStudioPlateResult {
                http,
                plate_index: -1,
            }
        }
        Recovery::Retry(retry) => {
            take_http(initial.http);
            retry.with_account(|account| pandar_plugin_studio_get_plate(account, task_id))
        }
    }
}

/// # Safety
///
/// `account` must point to a valid ABI value for the duration of this call. Any callback stored in
/// it or supplied through `with_current` must honor its declared pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_studio_get_subtask_with_session(
    session: *mut c_void,
    account: *const PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    task_id: PluginBytes,
) -> PluginHttpResult {
    let initial = pandar_plugin_studio_get_subtask(account, task_id);
    recover_http(
        initial,
        RecoveryContext {
            session,
            config_epoch,
            session_kind,
            account_context,
            with_current,
        },
        account,
        |retry| pandar_plugin_studio_get_subtask(retry, task_id),
    )
}

/// # Safety
///
/// `account` and its byte views must remain valid for this call. `visitor` must honor the model-task
/// pointer contract and copy any borrowed byte views before returning.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_studio_get_model_task_with_session(
    session: *mut c_void,
    account: *const PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    task_id: PluginBytes,
    visitor_context: *mut c_void,
    visitor: Option<StudioModelTaskVisitor>,
    cancellation_context: *mut c_void,
    cancelled: Option<StudioModelTaskCancelled>,
) -> PluginHttpResult {
    let cancellation = ModelTaskCancellation::new(cancellation_context, cancelled);
    let initial = get_model_task(account, task_id, visitor_context, visitor, cancellation);
    recover_model_task_http(
        initial,
        RecoveryContext {
            session,
            config_epoch,
            session_kind,
            account_context,
            with_current,
        },
        account,
        cancellation,
        |retry| get_model_task(retry, task_id, visitor_context, visitor, cancellation),
    )
}

fn recover_model_task_http(
    initial: PluginHttpResult,
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    cancellation: ModelTaskCancellation,
    retry: impl FnOnce(&PluginStudioAccount) -> PluginHttpResult,
) -> PluginHttpResult {
    recover_http_with(
        initial,
        || recovery_with_cancellation(context, account, cancellation),
        retry,
    )
}

fn recover_http(
    initial: PluginHttpResult,
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    retry: impl FnOnce(&PluginStudioAccount) -> PluginHttpResult,
) -> PluginHttpResult {
    recover_http_with(initial, || recovery(context, account), retry)
}

fn recover_http_with(
    initial: PluginHttpResult,
    recovery: impl FnOnce() -> Recovery,
    retry: impl FnOnce(&PluginStudioAccount) -> PluginHttpResult,
) -> PluginHttpResult {
    if !auth_rejected(&initial) {
        return initial;
    }
    match recovery() {
        Recovery::Original => initial,
        Recovery::Failure(http) => {
            take_http(initial);
            http
        }
        Recovery::Retry(account) => {
            take_http(initial);
            account.with_account(retry)
        }
    }
}

enum Recovery {
    Original,
    Failure(PluginHttpResult),
    Retry(BoundRetryAccount),
}

struct BoundRetryAccount {
    identity: NoAuthExpected,
    context: *mut c_void,
    current_snapshot: Option<StudioSnapshotCallback>,
}

impl BoundRetryAccount {
    fn new(identity: NoAuthExpected, account: &PluginStudioAccount) -> Self {
        Self {
            identity,
            context: account.context,
            current_snapshot: account.current_snapshot,
        }
    }

    fn with_account<T>(&self, retry: impl FnOnce(&PluginStudioAccount) -> T) -> T {
        let snapshot = PluginStudioSnapshot {
            hub_url: bytes(&self.identity.hub_url),
            token: bytes(&self.identity.token),
            printer_id: bytes(""),
            printer_authorized: 0,
            account_transition_pending: 0,
            account_epoch: self.identity.account_epoch,
            cache_generation: 0,
            firmware_generation: 0,
        };
        retry(&PluginStudioAccount {
            snapshot,
            context: self.context,
            current_snapshot: self.current_snapshot,
        })
    }
}

fn recovery(context: RecoveryContext, account: *const PluginStudioAccount) -> Recovery {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return Recovery::Original;
    };
    let expected = match expected(account, context.config_epoch, context.session_kind) {
        Some(expected) => expected,
        None => return Recovery::Original,
    };
    finish_recovery(
        account,
        recover(
            context.session,
            expected,
            context.account_context,
            context.with_current,
        ),
    )
}

fn recovery_with_cancellation(
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    cancellation: ModelTaskCancellation,
) -> Recovery {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return Recovery::Original;
    };
    let expected = match expected(account, context.config_epoch, context.session_kind) {
        Some(expected) => expected,
        None => return Recovery::Original,
    };
    finish_recovery(
        account,
        recover_account_with_cancellation(
            context.session,
            expected,
            context.account_context,
            context.with_current,
            cancellation,
        ),
    )
}

fn finish_recovery(account: &PluginStudioAccount, recovery: NoAuthRecovery) -> Recovery {
    match recovery {
        NoAuthRecovery::NotApplicable => Recovery::Original,
        NoAuthRecovery::Stale => {
            Recovery::Failure(super::tasks::failure_result(409, "stale_task_response"))
        }
        NoAuthRecovery::Failed(outcome) => Recovery::Failure(into_http(outcome)),
        NoAuthRecovery::Recovered(identity) => {
            Recovery::Retry(BoundRetryAccount::new(identity, account))
        }
    }
}

fn expected(
    account: &PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
) -> Option<NoAuthExpected> {
    Some(NoAuthExpected {
        hub_url: account.snapshot.hub_url.read("hub_url").ok()?,
        token: account.snapshot.token.read("token").ok()?,
        account_epoch: account.snapshot.account_epoch,
        config_epoch,
        session_kind,
    })
}

fn bytes(value: &str) -> PluginBytes {
    PluginBytes {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

fn auth_rejected(result: &PluginHttpResult) -> bool {
    result.status != 0 && matches!(result.http_code, 401 | 410)
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::c_void,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    mod recovered_account;

    struct SnapshotState {
        hub_url: String,
        token: String,
        account_epoch: u64,
    }

    extern "C" fn switched_account_snapshot(
        context: *mut c_void,
        snapshot: *mut PluginStudioSnapshot,
    ) -> i32 {
        let state = unsafe { &*(context.cast::<SnapshotState>()) };
        unsafe {
            *snapshot = PluginStudioSnapshot {
                hub_url: bytes(&state.hub_url),
                token: bytes(&state.token),
                printer_id: bytes(""),
                printer_authorized: 0,
                account_transition_pending: 0,
                account_epoch: state.account_epoch,
                cache_generation: 0,
                firmware_generation: 0,
            };
        }
        1
    }

    #[test]
    fn stale_finished_follower_returns_the_task_stale_response() {
        let hub_url = "http://hub";
        let token = "old-a-token";
        let account = PluginStudioAccount {
            snapshot: PluginStudioSnapshot {
                hub_url: bytes(hub_url),
                token: bytes(token),
                printer_id: bytes(""),
                printer_authorized: 0,
                account_transition_pending: 0,
                account_epoch: 7,
                cache_generation: 0,
                firmware_generation: 0,
            },
            context: std::ptr::null_mut(),
            current_snapshot: None,
        };

        let Recovery::Failure(result) = finish_recovery(&account, NoAuthRecovery::Stale) else {
            panic!("stale follower did not fail the task request");
        };
        let outcome = take_http(result);
        assert_eq!(outcome.status, 1);
        assert_eq!(outcome.http_code, 409);
        assert_eq!(outcome.body, r#"{"error":"stale_task_response"}"#);
    }
}
