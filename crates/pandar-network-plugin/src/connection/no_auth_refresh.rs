use crate::{
    PluginHttpResult,
    account::lifecycle::{PluginWithCurrentAccount, current_expected},
    invalid_input,
    plugin_core::{PluginCore, core as plugin_core},
    result, stable_error_body,
    studio_status::FirmwareProjection,
};
use std::ffi::c_void;
use std::time::Duration;

use super::{
    ConnectionSession, PluginConnectionResult, PluginPrinterRefreshAdapter,
    projection::CachedPrinterProjection,
};

const STUDIO_PRINT_INFO: i32 = 1;
const BACKGROUND_REFRESH: i32 = 2;

/// Bounded wait for the first account-fenced snapshot before the print-info
/// source gives up with `503 {"error":"cache_initializing"}`.
const PRINT_INFO_CACHE_WAIT: Duration = Duration::from_secs(30);

#[repr(C)]
pub struct PluginPrinterRefreshLifecycleResult {
    pub http: PluginHttpResult,
    pub connection: PluginConnectionResult,
    pub cache_committed: i32,
    pub snapshot_current: i32,
}

/// Serves the Studio print-info / background printer payload from the cached
/// stream projection only. The Hub is never contacted on this path.
#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore for the duration of the call. `account_context`,
/// `adapter.context`, and every supplied callback must remain valid for this synchronous call and
/// satisfy the callback-specific pointer contracts.
pub unsafe extern "C" fn pandar_plugin_core_printer_refresh(
    core_ptr: *mut c_void,
    mode: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    adapter: PluginPrinterRefreshAdapter,
) -> PluginPrinterRefreshLifecycleResult {
    let Some(core) = (unsafe { plugin_core(core_ptr) }) else {
        return failure(invalid_input("invalid_printer_refresh_session"));
    };
    let session = core.connection();
    if !matches!(mode, STUDIO_PRINT_INFO | BACKGROUND_REFRESH)
        || adapter.with_refresh_lock.is_none()
        || adapter.collect_offline.is_none()
    {
        return failure(invalid_input("invalid_printer_refresh_adapter"));
    }
    let expected = match current_expected(account_context, with_current) {
        Ok(expected) => expected,
        Err(error) => {
            eprintln!("pandar printer refresh account snapshot failed: {error:#}");
            return failure(invalid_input("account_state_unavailable"));
        }
    };

    let mut admission = Admission {
        session,
        account_epoch: expected.account_epoch,
        require_token: mode == STUDIO_PRINT_INFO,
        token_present: !expected.token.trim().is_empty(),
    };
    let admission_status = with_refresh_lock(
        adapter,
        (&mut admission as *mut Admission<'_>).cast(),
        begin_admission,
    );
    if admission_status != 0 {
        return failure(admission_failure(mode, admission_status, &expected.token));
    }
    let mut guard = AdmissionGuard::new(session);

    let served = match mode {
        STUDIO_PRINT_INFO => {
            session.wait_cached_printer_projection(expected.account_epoch, PRINT_INFO_CACHE_WAIT)
        }
        _ => session.cached_printer_projection(),
    };
    let Some(CachedPrinterProjection {
        body,
        firmware,
        printer_epoch,
    }) = served
    else {
        return failure(result(1, 503, stable_error_body("cache_initializing")));
    };

    let mut finalized = Finalization {
        core,
        session,
        adapter,
        account_epoch: expected.account_epoch,
        printer_epoch,
        firmware: Some(&firmware),
        connection: empty_connection_result(),
        snapshot_current: true,
    };
    if with_refresh_lock(
        adapter,
        (&mut finalized as *mut Finalization<'_>).cast(),
        finalize_serve,
    ) != 0
    {
        return failure(result(
            1,
            0,
            stable_error_body("printer_refresh_adapter_failed"),
        ));
    }
    guard.disarm();

    let snapshot_current = finalized.snapshot_current;
    let http = if mode == STUDIO_PRINT_INFO {
        crate::studio_policy::pandar_plugin_studio_print_info_result(
            0,
            200,
            body.as_ptr(),
            body.len(),
            snapshot_current,
        )
    } else {
        result(0, 200, body)
    };
    PluginPrinterRefreshLifecycleResult {
        http,
        connection: finalized.connection,
        cache_committed: i32::from(snapshot_current),
        snapshot_current: i32::from(snapshot_current),
    }
}

struct Admission<'a> {
    session: &'a ConnectionSession,
    account_epoch: u64,
    require_token: bool,
    token_present: bool,
}

unsafe extern "C" fn begin_admission(context: *mut c_void) -> i32 {
    let Some(admission) = (unsafe { context.cast::<Admission<'_>>().as_ref() }) else {
        return 1;
    };
    admission.session.begin_printer_cache_admission(
        admission.account_epoch,
        admission.require_token,
        admission.token_present,
    )
}

struct Finalization<'a> {
    core: &'a PluginCore,
    session: &'a ConnectionSession,
    adapter: PluginPrinterRefreshAdapter,
    account_epoch: u64,
    printer_epoch: u64,
    firmware: Option<&'a FirmwareProjection>,
    connection: PluginConnectionResult,
    snapshot_current: bool,
}

unsafe extern "C" fn finalize_serve(context: *mut c_void) -> i32 {
    let Some(finalization) = (unsafe { context.cast::<Finalization<'_>>().as_mut() }) else {
        return 1;
    };
    finalization.snapshot_current = finalization
        .session
        .printer_cache_snapshot_current(finalization.account_epoch, finalization.printer_epoch);
    if finalization.snapshot_current
        && let Some(projection) = finalization.firmware
        && let Err(error) = finalization.core.observe_firmware_projection(projection)
    {
        eprintln!("pandar firmware printer observation failed: {error:#}");
    }
    finalization.connection = finalization.session.take_transition();
    let collect_offline = finalization
        .adapter
        .collect_offline
        .expect("printer refresh offline collector was validated");
    for offline in finalization.session.take_offline() {
        collect_offline(
            finalization.adapter.context,
            offline.dev_id.as_ptr(),
            offline.dev_id.len(),
            offline.ticket,
        );
    }
    finalization.session.finish_printer_cache_admission();
    0
}

fn with_refresh_lock(
    adapter: PluginPrinterRefreshAdapter,
    context: *mut c_void,
    transaction: unsafe extern "C" fn(*mut c_void) -> i32,
) -> i32 {
    let with_refresh_lock = adapter
        .with_refresh_lock
        .expect("printer refresh lock adapter was validated");
    unsafe { with_refresh_lock(adapter.context, context, Some(transaction)) }
}

fn admission_failure(mode: i32, status: i32, token: &str) -> PluginHttpResult {
    if mode == STUDIO_PRINT_INFO {
        let (transition_pending, token) = if status == 2 {
            (false, "")
        } else {
            (true, token)
        };
        return crate::studio_policy::pandar_plugin_studio_print_info_admission(
            true,
            transition_pending,
            token.as_ptr(),
            token.len(),
        );
    }
    result(1, 409, stable_error_body("account_transition"))
}

struct AdmissionGuard<'a> {
    session: &'a ConnectionSession,
    armed: bool,
}

impl<'a> AdmissionGuard<'a> {
    fn new(session: &'a ConnectionSession) -> Self {
        Self {
            session,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for AdmissionGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.session.finish_printer_cache_admission();
        }
    }
}

fn failure(http: PluginHttpResult) -> PluginPrinterRefreshLifecycleResult {
    PluginPrinterRefreshLifecycleResult {
        http,
        connection: empty_connection_result(),
        cache_committed: 0,
        snapshot_current: 0,
    }
}

fn empty_connection_result() -> PluginConnectionResult {
    PluginConnectionResult {
        status: 0,
        http_code: 0,
        connected: 0,
        changed: 0,
        auth_rejected: 0,
        auth_changed: 0,
        transition_ticket: 0,
        auth_ticket: 0,
    }
}
