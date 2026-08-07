use std::ffi::c_void;

use crate::{
    PluginHttpResult,
    account::lifecycle::{
        NoAuthRecovery, PluginWithCurrentAccount, current_expected, into_http, recover, take_http,
    },
    firmware::session_ref as firmware_session,
    invalid_input, result, stable_error_body,
    studio_status::FirmwareProjection,
};

use super::{
    ConnectionSession, PluginConnectionResult, PluginPrinterRefreshAdapter, PrinterRefreshResult,
    ffi::session, no_auth_rotation::NoAuthRotationOutcome,
};

const STUDIO_PRINT_INFO: i32 = 1;
const BACKGROUND_REFRESH: i32 = 2;

#[repr(C)]
pub struct PluginPrinterRefreshLifecycleResult {
    pub http: PluginHttpResult,
    pub connection: PluginConnectionResult,
    pub cache_committed: i32,
    pub snapshot_current: i32,
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_refresh_with_session(
    session_ptr: *mut c_void,
    mode: i32,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    adapter: PluginPrinterRefreshAdapter,
) -> PluginPrinterRefreshLifecycleResult {
    let Some(session) = session(session_ptr) else {
        return failure(invalid_input("invalid_printer_refresh_session"));
    };
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
    let initial_epoch = expected.account_epoch;
    let mut admission = Admission {
        session,
        account_epoch: initial_epoch,
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

    let (refresh, account_epoch) = refresh_with_recovery(
        session_ptr,
        session,
        expected,
        account_context,
        with_current,
        adapter,
        mode == STUDIO_PRINT_INFO,
    );
    let (snapshot_current, connection) = {
        let mut finalized = Finalization {
            session,
            adapter,
            account_epoch,
            upstream: &refresh.upstream,
            firmware: refresh.firmware.as_ref(),
            connection: empty_connection_result(),
            snapshot_current: true,
        };
        if with_refresh_lock(
            adapter,
            (&mut finalized as *mut Finalization<'_>).cast(),
            finalize_refresh,
        ) != 0
        {
            return failure(result(
                1,
                0,
                stable_error_body("printer_refresh_adapter_failed"),
            ));
        }
        guard.disarm();
        (finalized.snapshot_current, finalized.connection)
    };
    let cache_committed = refresh.upstream.status == 0 && snapshot_current;
    let http = if mode == STUDIO_PRINT_INFO {
        crate::studio_policy::pandar_plugin_studio_print_info_result(
            refresh.upstream.status,
            refresh.upstream.http_code,
            refresh.upstream.body.as_ptr(),
            refresh.upstream.body.len(),
            snapshot_current,
        )
    } else {
        into_http(refresh.upstream)
    };
    PluginPrinterRefreshLifecycleResult {
        http,
        connection,
        cache_committed: i32::from(cache_committed),
        snapshot_current: i32::from(snapshot_current),
    }
}

fn refresh_with_recovery(
    session_ptr: *mut c_void,
    session: &ConnectionSession,
    expected: crate::account::lifecycle::NoAuthExpected,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
    adapter: PluginPrinterRefreshAdapter,
    invalidate_freshness: bool,
) -> (ProjectedRefresh, u64) {
    let initial_epoch = expected.account_epoch;
    let initial = take_projected(refresh(session, adapter, None, invalidate_freshness));
    if !auth_rejected(&initial) {
        return (initial, initial_epoch);
    }
    match recover(session_ptr, expected, account_context, with_current) {
        NoAuthRecovery::NotApplicable => (initial, initial_epoch),
        NoAuthRecovery::Stale => (
            ProjectedRefresh::without_firmware(NoAuthRotationOutcome {
                status: 1,
                http_code: 409,
                body: stable_error_body("stale_no_auth_session"),
            }),
            initial_epoch,
        ),
        NoAuthRecovery::Failed(outcome) => {
            (ProjectedRefresh::without_firmware(outcome), initial_epoch)
        }
        NoAuthRecovery::Recovered(identity) => (
            take_projected(refresh(
                session,
                adapter,
                Some(&identity),
                invalidate_freshness,
            )),
            identity.account_epoch,
        ),
    }
}

fn refresh(
    session: &ConnectionSession,
    adapter: PluginPrinterRefreshAdapter,
    expected: Option<&crate::account::lifecycle::NoAuthExpected>,
    invalidate_freshness: bool,
) -> PrinterRefreshResult {
    session.refresh_printers(
        expected.map(|expected| {
            (
                expected.hub_url.as_str(),
                expected.token.as_str(),
                expected.account_epoch,
            )
        }),
        invalidate_freshness,
        || {
            if let Some(reserve_observation) = adapter.reserve_observation {
                reserve_observation(adapter.context);
            }
        },
    )
}

fn auth_rejected(result: &ProjectedRefresh) -> bool {
    result.upstream.status != 0 && matches!(result.upstream.http_code, 401 | 410)
}

struct ProjectedRefresh {
    upstream: NoAuthRotationOutcome,
    firmware: Option<FirmwareProjection>,
}

impl ProjectedRefresh {
    fn without_firmware(upstream: NoAuthRotationOutcome) -> Self {
        Self {
            upstream,
            firmware: None,
        }
    }
}

fn take_projected(refresh: PrinterRefreshResult) -> ProjectedRefresh {
    ProjectedRefresh {
        upstream: take_http(refresh.http),
        firmware: refresh.firmware,
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
    session: &'a ConnectionSession,
    adapter: PluginPrinterRefreshAdapter,
    account_epoch: u64,
    upstream: &'a NoAuthRotationOutcome,
    firmware: Option<&'a FirmwareProjection>,
    connection: PluginConnectionResult,
    snapshot_current: bool,
}

unsafe extern "C" fn finalize_refresh(context: *mut c_void) -> i32 {
    let Some(finalization) = (unsafe { context.cast::<Finalization<'_>>().as_mut() }) else {
        return 1;
    };
    let failed = finalization.upstream.status != 0;
    finalization.snapshot_current = failed
        || finalization
            .session
            .printer_cache_snapshot_current(finalization.account_epoch);
    if !failed
        && finalization.snapshot_current
        && let Some(projection) = finalization.firmware
        && let Some(with_firmware_observation) = finalization.adapter.with_firmware_observation
    {
        let status = unsafe {
            with_firmware_observation(
                finalization.adapter.context,
                std::ptr::from_ref(projection).cast_mut().cast(),
                Some(observe_firmware_projection),
            )
        };
        if status != 0 {
            eprintln!("pandar firmware projection handoff failed");
        }
    }

    unsafe extern "C" fn observe_firmware_projection(
        projection_context: *mut c_void,
        session_ptr: *mut c_void,
        generation: u64,
        observation_sequence: u64,
    ) -> i32 {
        let Some(projection) =
            (unsafe { projection_context.cast::<FirmwareProjection>().as_ref() })
        else {
            return 1;
        };
        let Some(session) = (unsafe { firmware_session(session_ptr) }) else {
            return 1;
        };
        if let Err(error) = session.observe_printers(projection, generation, observation_sequence) {
            eprintln!("pandar firmware printer observation failed: {error:#}");
        }
        0
    }
    if failed || finalization.snapshot_current {
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
