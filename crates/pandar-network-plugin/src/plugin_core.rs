use std::{
    ffi::c_void,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    account::{
        lifecycle::{PluginLifecycleResult, transaction::PluginWithCurrentAccount},
        session::{
            AccountLifecycleSession, PluginAccountSessionBridge,
            callbacks::pandar_plugin_account_session_drain, pandar_plugin_account_session_apply,
            pandar_plugin_account_session_apply_lifecycle_result,
        },
    },
    connection::ConnectionSession,
    dispatch::PluginDispatchBridge,
    firmware::FirmwarePluginSession,
    normalize_hub_url,
    personal_presets::pandar_plugin_personal_preset_reset,
    read_utf8,
    studio_policy::login_observation::{
        pandar_plugin_account_identity_create, pandar_plugin_account_login_observation_clear,
    },
    studio_print::{PluginStudioSnapshot, pandar_plugin_studio_request_snapshot_current},
    studio_status::FirmwareProjection,
};

pub(crate) struct PluginCore {
    account: AccountLifecycleSession,
    connection: ConnectionSession,
    firmware: FirmwarePluginSession,
    account_identity: u64,
    firmware_observation_sequence: AtomicU64,
}

impl PluginCore {
    fn new(hub_url: String, token: String) -> Self {
        Self {
            account: AccountLifecycleSession::new(),
            connection: ConnectionSession::new(hub_url.clone(), token.clone()),
            firmware: FirmwarePluginSession::new(hub_url, token, 1),
            account_identity: pandar_plugin_account_identity_create(),
            firmware_observation_sequence: AtomicU64::new(0),
        }
    }

    fn connection_ptr(&self) -> *mut c_void {
        std::ptr::from_ref(&self.connection).cast_mut().cast()
    }

    fn firmware_ptr(&self) -> *mut c_void {
        std::ptr::from_ref(&self.firmware).cast_mut().cast()
    }

    fn account_ptr(&self) -> *mut c_void {
        std::ptr::from_ref(&self.account).cast_mut().cast()
    }

    pub(crate) fn connection(&self) -> &ConnectionSession {
        &self.connection
    }

    pub(crate) fn observe_firmware_projection(
        &self,
        projection: &FirmwareProjection,
    ) -> anyhow::Result<()> {
        let observation = self.reserve_firmware_observation();
        self.firmware
            .observe_printers(projection, observation.generation, observation.sequence)
    }

    fn reserve_firmware_observation(&self) -> FirmwareObservation {
        FirmwareObservation {
            generation: self.firmware.generation(),
            sequence: self
                .firmware_observation_sequence
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1),
        }
    }
}

impl Drop for PluginCore {
    fn drop(&mut self) {
        self.connection.stop_worker();
        self.firmware.stop();
        pandar_plugin_personal_preset_reset(self.account_identity);
        pandar_plugin_account_login_observation_clear(self.account_identity);
    }
}

struct FirmwareObservation {
    generation: u64,
    sequence: u64,
}

pub(crate) unsafe fn core<'a>(core: *mut c_void) -> Option<&'a PluginCore> {
    unsafe { core.cast::<PluginCore>().as_ref() }
}

#[unsafe(no_mangle)]
/// # Safety
/// Input pointers must be valid for their corresponding lengths. A non-null return is one
/// caller-owned core that must be destroyed exactly once with `pandar_plugin_core_destroy` after
/// all borrowed component pointers and dispatcher threads are finished.
pub unsafe extern "C" fn pandar_plugin_core_create(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> *mut c_void {
    let Some(hub_url) = unsafe { read_utf8(hub_url_ptr, hub_url_len) }.and_then(normalize_hub_url)
    else {
        return std::ptr::null_mut();
    };
    let Some(token) = (unsafe { read_utf8(token_ptr, token_len) }) else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(PluginCore::new(hub_url, token))).cast()
}

#[unsafe(no_mangle)]
/// # Safety
/// `core` must be null or returned by `pandar_plugin_core_create` exactly once. All borrowed
/// component calls and C++ dispatcher threads must have completed before destruction.
pub unsafe extern "C" fn pandar_plugin_core_destroy(core: *mut c_void) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core.cast::<PluginCore>()) });
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore. The returned connection pointer is borrowed; it must
/// never be destroyed and must not be used after the core is destroyed.
pub unsafe extern "C" fn pandar_plugin_core_connection_session(
    core_ptr: *mut c_void,
) -> *mut c_void {
    unsafe { core(core_ptr) }.map_or(std::ptr::null_mut(), PluginCore::connection_ptr)
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore. The returned firmware pointer is borrowed; it must
/// never be destroyed and must not be used after the core is destroyed.
pub unsafe extern "C" fn pandar_plugin_core_firmware_session(core_ptr: *mut c_void) -> *mut c_void {
    unsafe { core(core_ptr) }.map_or(std::ptr::null_mut(), PluginCore::firmware_ptr)
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore.
pub unsafe extern "C" fn pandar_plugin_core_account_identity(core_ptr: *mut c_void) -> u64 {
    unsafe { core(core_ptr) }.map_or(0, |core| core.account_identity)
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore for the duration of the call.
pub unsafe extern "C" fn pandar_plugin_core_printer_request_snapshot_current(
    core_ptr: *mut c_void,
    account_epoch: u64,
    cache_generation: u64,
    firmware_generation: u64,
) -> i32 {
    unsafe { core(core_ptr) }.is_some_and(|core| {
        core.connection
            .studio_request_snapshot_current(account_epoch, cache_generation)
            && core.firmware.generation_is_current(firmware_generation)
    }) as i32
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore for the duration of the call.
pub unsafe extern "C" fn pandar_plugin_core_sync_firmware(core_ptr: *mut c_void) -> i32 {
    let Some(core) = (unsafe { core(core_ptr) }) else {
        return 1;
    };
    let observation = core.reserve_firmware_observation();
    let Some(projection) = core.connection.cached_firmware_projection() else {
        return 1;
    };
    match core
        .firmware
        .observe_printers(&projection, observation.generation, observation.sequence)
    {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("pandar streamed firmware projection failed: {error:#}");
            1
        }
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must point to a live PluginCore and both snapshots and their byte views must remain
/// valid for this synchronous call.
pub unsafe extern "C" fn pandar_plugin_core_studio_request_snapshot_current(
    core_ptr: *mut c_void,
    expected: *const PluginStudioSnapshot,
    current: *const PluginStudioSnapshot,
) -> i32 {
    let Some((core, expected)) = (unsafe { core(core_ptr) }).zip(unsafe { expected.as_ref() })
    else {
        return 0;
    };
    if unsafe { pandar_plugin_studio_request_snapshot_current(expected, current) } == 0 {
        return 0;
    }
    i32::from(
        core.connection
            .studio_request_snapshot_current(expected.account_epoch, expected.cache_generation)
            && core
                .firmware
                .generation_is_current(expected.firmware_generation),
    )
}

#[unsafe(no_mangle)]
/// # Safety
/// `core`, `lifecycle`, and all lifecycle allocations must remain live for the call.
pub unsafe extern "C" fn pandar_plugin_core_account_apply_lifecycle_result(
    core_ptr: *mut c_void,
    lifecycle: *const PluginLifecycleResult,
) {
    let Some(core) = (unsafe { core(core_ptr) }) else {
        return;
    };
    unsafe { pandar_plugin_account_session_apply_lifecycle_result(core.account_ptr(), lifecycle) };
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must identify a live PluginCore for the call. Every bridge, account view, mutation,
/// and C++ context pointer must also remain live for the call.
pub unsafe extern "C" fn pandar_plugin_core_account_apply(
    core_ptr: *mut c_void,
    bridge: *const PluginAccountSessionBridge,
    agent: *mut c_void,
    current: *const crate::account::lifecycle::transaction::PluginAccountView,
    mutation: *const crate::account::lifecycle::transaction::PluginAccountMutation,
) -> i32 {
    let Some(core) = (unsafe { core(core_ptr) }) else {
        return 1;
    };
    unsafe {
        pandar_plugin_account_session_apply(
            core.account_ptr(),
            core.connection_ptr(),
            core.firmware_ptr(),
            bridge,
            agent,
            current,
            mutation,
        )
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `core_ptr` must identify a live PluginCore for the call. Every bridge and C++ context pointer
/// must also remain live for the call.
pub unsafe extern "C" fn pandar_plugin_core_account_drain(
    core_ptr: *mut c_void,
    dispatch_bridge: *const PluginDispatchBridge,
    account_bridge: *const PluginAccountSessionBridge,
    agent: *mut c_void,
    account_context: *mut c_void,
    with_current: Option<PluginWithCurrentAccount>,
) {
    let Some(core) = (unsafe { core(core_ptr) }) else {
        return;
    };
    unsafe {
        pandar_plugin_account_session_drain(
            core.account_ptr(),
            core.connection_ptr(),
            dispatch_bridge,
            account_bridge,
            agent,
            account_context,
            with_current,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_owns_stable_component_addresses_and_freshness() {
        let hub = b"http://127.0.0.1:8080";
        let token = b"token";
        let core = unsafe {
            pandar_plugin_core_create(hub.as_ptr(), hub.len(), token.as_ptr(), token.len())
        };
        assert!(!core.is_null());
        let connection = unsafe { pandar_plugin_core_connection_session(core) };
        let firmware = unsafe { pandar_plugin_core_firmware_session(core) };
        assert!(!connection.is_null());
        assert!(!firmware.is_null());
        assert_eq!(connection, unsafe {
            pandar_plugin_core_connection_session(core)
        });
        assert_eq!(firmware, unsafe {
            pandar_plugin_core_firmware_session(core)
        });
        assert_ne!(unsafe { pandar_plugin_core_account_identity(core) }, 0);
        let core_ref = unsafe { super::core(core) }.unwrap();
        let first = core_ref.reserve_firmware_observation();
        let second = core_ref.reserve_firmware_observation();
        assert_eq!(first.generation, 1);
        assert_eq!(first.sequence, 1);
        assert_eq!(second.generation, 1);
        assert_eq!(second.sequence, 2);
        let (snapshot, _) = unsafe { super::core(core) }
            .unwrap()
            .connection
            .studio_request_snapshot("printer".to_owned());
        assert_eq!(
            unsafe {
                pandar_plugin_core_printer_request_snapshot_current(
                    core,
                    snapshot.account_epoch,
                    snapshot.cache_generation,
                    1,
                )
            },
            1
        );
        assert_eq!(
            unsafe {
                pandar_plugin_core_printer_request_snapshot_current(
                    core,
                    snapshot.account_epoch,
                    snapshot.cache_generation,
                    2,
                )
            },
            0
        );
        let empty = crate::studio_print::PluginBytes {
            ptr: std::ptr::null(),
            len: 0,
        };
        let current = PluginStudioSnapshot {
            hub_url: empty,
            token: empty,
            printer_id: empty,
            printer_authorized: 1,
            account_transition_pending: 0,
            account_epoch: snapshot.account_epoch,
            cache_generation: snapshot.cache_generation,
            firmware_generation: 1,
        };
        assert_eq!(
            unsafe { pandar_plugin_core_studio_request_snapshot_current(core, &current, &current) },
            1
        );
        let stale_firmware = PluginStudioSnapshot {
            firmware_generation: 2,
            ..current
        };
        assert_eq!(
            unsafe {
                pandar_plugin_core_studio_request_snapshot_current(
                    core,
                    &stale_firmware,
                    &stale_firmware,
                )
            },
            0
        );
        assert!(
            !unsafe { super::core(core) }
                .unwrap()
                .connection
                .is_logged_out()
        );
        unsafe { pandar_plugin_core_destroy(core) };

        let logged_out = PluginCore::new("http://127.0.0.1:8080".to_owned(), String::new());
        assert!(logged_out.connection.is_logged_out());
    }
}
