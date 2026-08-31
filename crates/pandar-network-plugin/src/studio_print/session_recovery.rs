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
/// `session` must be live; `account`, `query`, and their nested byte views must remain valid for
/// this call. Account snapshot and `with_current` callbacks plus their contexts must remain valid
/// and callable from runtime worker threads until the call returns. Snapshot callback byte views
/// must stay readable until this function returns; transaction-view bytes must remain valid until
/// their transaction callback returns.
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
    unsafe {
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
            |retry| pandar_plugin_studio_get_tasks(retry, query),
        )
    }
}

/// # Safety
///
/// `session` must be live; `account`, `task_id`, and nested byte views must remain valid for this
/// call. Account snapshot and `with_current` callbacks plus their contexts must remain valid and
/// callable from runtime worker threads until the call returns. Snapshot callback byte views must
/// stay readable until this function returns; transaction-view bytes must remain valid until their
/// transaction callback returns.
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
    let initial = unsafe { pandar_plugin_studio_get_plate(account, task_id) };
    if !auth_rejected(&initial.http) {
        return initial;
    }
    match unsafe {
        recovery(
            RecoveryContext {
                session,
                config_epoch,
                session_kind,
                account_context,
                with_current,
            },
            account,
        )
    } {
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
            retry
                .with_account(|account| unsafe { pandar_plugin_studio_get_plate(account, task_id) })
        }
    }
}

/// # Safety
///
/// `session` must be live; `account`, `task_id`, and nested byte views must remain valid for this
/// call. Account snapshot and `with_current` callbacks plus their contexts must remain valid and
/// callable from runtime worker threads until the call returns. Snapshot callback byte views must
/// stay readable until this function returns; transaction-view bytes must remain valid until their
/// transaction callback returns.
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
    let initial = unsafe { pandar_plugin_studio_get_subtask(account, task_id) };
    unsafe {
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
}

/// # Safety
///
/// `session` must identify a live connection session. `account`, `task_id`, and every nested byte
/// view must remain valid for this synchronous call. `account_context`/`with_current`,
/// `visitor_context`/`visitor`, and `cancellation_context`/`cancelled` must remain valid and safe to
/// invoke from runtime worker threads until the call returns; visitors must copy borrowed byte
/// views before returning. Snapshot callback byte views must stay readable until this function
/// returns; transaction-view bytes must remain valid until their transaction callback returns.
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
    let initial =
        unsafe { get_model_task(account, task_id, visitor_context, visitor, cancellation) };
    unsafe {
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
}

unsafe fn recover_model_task_http(
    initial: PluginHttpResult,
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    cancellation: ModelTaskCancellation,
    retry: impl FnOnce(&PluginStudioAccount) -> PluginHttpResult,
) -> PluginHttpResult {
    recover_http_with(
        initial,
        || unsafe { recovery_with_cancellation(context, account, cancellation) },
        retry,
    )
}

unsafe fn recover_http(
    initial: PluginHttpResult,
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    retry: impl FnOnce(&PluginStudioAccount) -> PluginHttpResult,
) -> PluginHttpResult {
    recover_http_with(initial, || unsafe { recovery(context, account) }, retry)
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

unsafe fn recovery(context: RecoveryContext, account: *const PluginStudioAccount) -> Recovery {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return Recovery::Original;
    };
    let expected = match unsafe { expected(account, context.config_epoch, context.session_kind) } {
        Some(expected) => expected,
        None => return Recovery::Original,
    };
    finish_recovery(account, unsafe {
        recover(
            context.session,
            expected,
            context.account_context,
            context.with_current,
        )
    })
}

unsafe fn recovery_with_cancellation(
    context: RecoveryContext,
    account: *const PluginStudioAccount,
    cancellation: ModelTaskCancellation,
) -> Recovery {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return Recovery::Original;
    };
    let expected = match unsafe { expected(account, context.config_epoch, context.session_kind) } {
        Some(expected) => expected,
        None => return Recovery::Original,
    };
    finish_recovery(account, unsafe {
        recover_account_with_cancellation(
            context.session,
            expected,
            context.account_context,
            context.with_current,
            cancellation,
        )
    })
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

unsafe fn expected(
    account: &PluginStudioAccount,
    config_epoch: u64,
    session_kind: i32,
) -> Option<NoAuthExpected> {
    Some(NoAuthExpected {
        hub_url: unsafe { account.snapshot.hub_url.read("hub_url") }.ok()?,
        token: unsafe { account.snapshot.token.read("token") }.ok()?,
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
mod tests;
