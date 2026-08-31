use std::{ffi::c_void, slice};

use super::{
    admission::{PrintFailure, admit, load_config_metadata},
    freshness::{self, AccountFreshness},
    lifecycle,
    tasks::{self, StudioAccount},
};
use crate::PluginHttpResult;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl PluginBytes {
    const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub(super) unsafe fn read(self, field: &'static str) -> Result<String, PrintFailure> {
        if self.len == 0 {
            return Ok(String::new());
        }
        if self.ptr.is_null() {
            return Err(PrintFailure::invalid(field));
        }
        let bytes = unsafe { slice::from_raw_parts(self.ptr, self.len) };
        std::str::from_utf8(bytes)
            .map(ToOwned::to_owned)
            .map_err(|_| PrintFailure::invalid(field))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginStudioSnapshot {
    pub hub_url: PluginBytes,
    pub token: PluginBytes,
    pub printer_id: PluginBytes,
    pub printer_authorized: u8,
    pub account_transition_pending: u8,
    pub account_epoch: u64,
    pub cache_generation: u64,
    pub firmware_generation: u64,
}

impl PluginStudioSnapshot {
    pub(super) const fn empty() -> Self {
        Self {
            hub_url: PluginBytes::empty(),
            token: PluginBytes::empty(),
            printer_id: PluginBytes::empty(),
            printer_authorized: 0,
            account_transition_pending: 0,
            account_epoch: 0,
            cache_generation: 0,
            firmware_generation: 0,
        }
    }
}

#[repr(C)]
pub struct PluginStudioPrintParams {
    pub snapshot: PluginStudioSnapshot,
    pub dev_id: PluginBytes,
    pub task_name: PluginBytes,
    pub project_name: PluginBytes,
    pub preset_name: PluginBytes,
    pub filename: PluginBytes,
    pub config_filename: PluginBytes,
    pub plate_index: i32,
    pub ftp_folder: PluginBytes,
    pub ftp_file: PluginBytes,
    pub ftp_file_md5: PluginBytes,
    pub nozzle_mapping: PluginBytes,
    pub ams_mapping: PluginBytes,
    pub ams_mapping2: PluginBytes,
    pub ams_mapping_info: PluginBytes,
    pub nozzles_info: PluginBytes,
    pub connection_type: PluginBytes,
    pub comments: PluginBytes,
    pub origin_profile_id: i32,
    pub stl_design_id: i32,
    pub origin_model_id: PluginBytes,
    pub print_type: PluginBytes,
    pub dst_file: PluginBytes,
    pub dev_name: PluginBytes,
    pub dev_ip: PluginBytes,
    pub use_ssl_for_ftp: u8,
    pub use_ssl_for_mqtt: u8,
    pub username: PluginBytes,
    pub password: PluginBytes,
    pub task_bed_leveling: u8,
    pub task_flow_cali: u8,
    pub task_vibration_cali: u8,
    pub task_layer_inspect: u8,
    pub task_record_timelapse: u8,
    pub task_timelapse_use_internal: u8,
    pub task_use_ams: u8,
    pub task_bed_type: PluginBytes,
    pub extra_options: PluginBytes,
    pub auto_bed_leveling: i32,
    pub auto_flow_cali: i32,
    pub auto_offset_cali: i32,
    pub extruder_cali_manual_mode: i32,
    pub task_ext_change_assist: u8,
    pub try_emmc_print: u8,
    pub svc_context: PluginBytes,
    pub slicer_uid: PluginBytes,
}

pub type StudioUpdateCallback = extern "C" fn(*mut c_void, i32, i32, *const u8, usize);
pub type StudioBooleanCallback = extern "C" fn(*mut c_void) -> i32;
pub type StudioWaitCallback = extern "C" fn(*mut c_void, i32, *const u8, usize) -> i32;
pub type StudioSnapshotCallback = extern "C" fn(*mut c_void, *mut PluginStudioSnapshot) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginStudioCallbacks {
    pub context: *mut c_void,
    pub update: Option<StudioUpdateCallback>,
    pub cancelled: Option<StudioBooleanCallback>,
    pub wait: Option<StudioWaitCallback>,
    pub snapshot: Option<StudioSnapshotCallback>,
}

unsafe impl Send for PluginStudioCallbacks {}
unsafe impl Sync for PluginStudioCallbacks {}

impl PluginStudioCallbacks {
    pub(super) fn update(self, stage: i32, code: i32, body: &str) {
        if let Some(callback) = self.update {
            callback(self.context, stage, code, body.as_ptr(), body.len());
        }
    }

    pub(super) fn error(self, failure: &PrintFailure) {
        self.update(7, failure.code, &failure.body);
    }

    pub(super) fn cancelled(self) -> bool {
        self.cancelled
            .is_some_and(|callback| callback(self.context) != 0)
    }

    pub(super) fn wait(self, body: &str) -> bool {
        self.wait
            .is_none_or(|callback| callback(self.context, 0, body.as_ptr(), body.len()) != 0)
    }

    pub(super) fn snapshot_current(self, print: &super::admission::AdmittedPrint) -> bool {
        let Some(callback) = self.snapshot else {
            return true;
        };
        let mut snapshot = PluginStudioSnapshot::empty();
        callback(self.context, &mut snapshot) != 0 && unsafe { print.matches_snapshot(&snapshot) }
    }
}

#[repr(C)]
pub struct PluginStudioAccount {
    pub snapshot: PluginStudioSnapshot,
    pub context: *mut c_void,
    pub current_snapshot: Option<StudioSnapshotCallback>,
}

#[repr(C)]
pub struct PluginStudioTaskQuery {
    pub dev_id: PluginBytes,
    pub status: i32,
    pub offset: i32,
    pub limit: i32,
}

#[repr(C)]
pub struct PluginStudioPlateResult {
    pub http: PluginHttpResult,
    pub plate_index: i32,
}

#[unsafe(no_mangle)]
/// # Safety
/// `params` and every nested byte view must remain valid until this synchronous call returns.
/// Every callback in `callbacks` and its `context` must remain valid, reentrancy-safe, and callable
/// for the full operation; callback consumers must copy borrowed byte views before returning. A
/// successful snapshot callback must populate nested byte views that remain readable until this
/// function returns.
pub unsafe extern "C" fn pandar_plugin_studio_start_print(
    params: *const PluginStudioPrintParams,
    callbacks: PluginStudioCallbacks,
) -> i32 {
    if callbacks.cancelled() {
        return -18;
    }
    let Some(params) = (unsafe { params.as_ref() }) else {
        let failure = PrintFailure::simple("invalid_print_param");
        callbacks.error(&failure);
        return failure.code;
    };
    let mut print = match unsafe { admit(params) } {
        Ok(print) => print,
        Err(failure) => {
            callbacks.error(&failure);
            return failure.code;
        }
    };
    if let Err(failure) = load_config_metadata(&mut print) {
        callbacks.error(&failure);
        return failure.code;
    }
    lifecycle::start(print, callbacks)
}

#[unsafe(no_mangle)]
/// # Safety
/// `account`, `query`, and their byte views must remain valid until this synchronous call returns.
/// `account.current_snapshot` and its context must remain callable for the full operation; every
/// successful callback result must contain nested byte views readable until this function returns.
pub unsafe extern "C" fn pandar_plugin_studio_get_tasks(
    account: *const PluginStudioAccount,
    query: *const PluginStudioTaskQuery,
) -> PluginHttpResult {
    let Some(query) = (unsafe { query.as_ref() }) else {
        return tasks::failure_result(400, "invalid_task_query");
    };
    let account = match unsafe { account_from_ptr(account) } {
        Ok(account) => account,
        Err(result) => return result,
    };
    let dev_id = match unsafe { query.dev_id.read("dev_id") } {
        Ok(dev_id) => dev_id,
        Err(_) => return tasks::failure_result(400, "invalid_task_query"),
    };
    tasks::get_tasks(account, dev_id, query.status, query.offset, query.limit)
}

#[unsafe(no_mangle)]
/// # Safety
/// `account`, `task_id`, and nested byte views must remain valid for this call. The account snapshot
/// callback/context must remain callable, and successful callback byte views readable, until return.
pub unsafe extern "C" fn pandar_plugin_studio_get_plate(
    account: *const PluginStudioAccount,
    task_id: PluginBytes,
) -> PluginStudioPlateResult {
    let account = match unsafe { account_from_ptr(account) } {
        Ok(account) => account,
        Err(http) => {
            return PluginStudioPlateResult {
                http,
                plate_index: -1,
            };
        }
    };
    let task_id = match unsafe { task_id.read("task_id") } {
        Ok(task_id) => task_id,
        Err(_) => {
            return PluginStudioPlateResult {
                http: tasks::failure_result(400, "invalid_task_id"),
                plate_index: -1,
            };
        }
    };
    tasks::get_plate(account, task_id)
}

#[unsafe(no_mangle)]
/// # Safety
/// `account`, `task_id`, and nested byte views must remain valid for this call. The account snapshot
/// callback/context must remain callable, and successful callback byte views readable, until return.
pub unsafe extern "C" fn pandar_plugin_studio_get_subtask(
    account: *const PluginStudioAccount,
    task_id: PluginBytes,
) -> PluginHttpResult {
    let account = match unsafe { account_from_ptr(account) } {
        Ok(account) => account,
        Err(result) => return result,
    };
    let task_id = match unsafe { task_id.read("task_id") } {
        Ok(task_id) => task_id,
        Err(_) => return tasks::failure_result(400, "invalid_task_id"),
    };
    tasks::get_subtask(account, task_id)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_slice_unavailable() -> PluginHttpResult {
    tasks::failure_result(501, "slice_info_unavailable")
}

#[unsafe(no_mangle)]
/// # Safety
/// Both snapshots and their byte views must remain valid for this synchronous call.
pub unsafe extern "C" fn pandar_plugin_studio_request_snapshot_current(
    expected: *const PluginStudioSnapshot,
    current: *const PluginStudioSnapshot,
) -> i32 {
    let Some((expected, current)) = (unsafe { expected.as_ref() }).zip(unsafe { current.as_ref() })
    else {
        return 0;
    };
    i32::from(unsafe { freshness::request_snapshot_current(expected, current) })
}

pub(super) unsafe fn account_from_ptr(
    account: *const PluginStudioAccount,
) -> Result<StudioAccount, PluginHttpResult> {
    let Some(account) = (unsafe { account.as_ref() }) else {
        return Err(tasks::failure_result(400, "invalid_auth_token"));
    };
    let freshness = unsafe {
        AccountFreshness::from_snapshot(
            &account.snapshot,
            account.context,
            account.current_snapshot,
        )
    }
    .ok_or_else(|| tasks::failure_result(409, "stale_task_response"))?;
    let hub_url = unsafe { account.snapshot.hub_url.read("hub_url") }
        .ok()
        .and_then(crate::normalize_hub_url)
        .ok_or_else(|| tasks::failure_result(400, "invalid_hub_url"))?;
    let token = unsafe { account.snapshot.token.read("token") }
        .map_err(|_| tasks::failure_result(401, "invalid_auth_token"))?;
    if token.trim().is_empty() {
        return Err(tasks::failure_result(401, "invalid_auth_token"));
    }
    Ok(StudioAccount {
        hub_url,
        token,
        freshness,
    })
}
