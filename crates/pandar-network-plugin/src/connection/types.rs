use std::ffi::c_void;

use crate::{PluginHttpResult, studio_status::FirmwareProjection};

pub type PrinterRefreshObservationReservation = extern "C" fn(*mut c_void);
pub type PrinterRefreshTransaction = unsafe extern "C" fn(*mut c_void) -> i32;
pub type PrinterRefreshWithLock =
    unsafe extern "C" fn(*mut c_void, *mut c_void, Option<PrinterRefreshTransaction>) -> i32;
pub type PrinterRefreshFirmwareTransaction =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u64, u64) -> i32;
pub type PrinterRefreshWithFirmware = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    Option<PrinterRefreshFirmwareTransaction>,
) -> i32;
pub type ConnectionPrinterVisitor = extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    i32,
);
pub type ConnectionDeviceVisitor = extern "C" fn(*mut c_void, *const u8, usize, u64);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginPrinterRefreshAdapter {
    pub context: *mut c_void,
    pub with_refresh_lock: Option<PrinterRefreshWithLock>,
    pub reserve_observation: Option<PrinterRefreshObservationReservation>,
    pub with_firmware_observation: Option<PrinterRefreshWithFirmware>,
    pub collect_offline: Option<ConnectionDeviceVisitor>,
}

pub(super) struct PrinterRefreshResult {
    pub(super) http: PluginHttpResult,
    pub(super) firmware: Option<FirmwareProjection>,
}

impl PrinterRefreshResult {
    pub(super) fn without_firmware(http: PluginHttpResult) -> Self {
        Self {
            http,
            firmware: None,
        }
    }

    pub(super) fn projected(http: PluginHttpResult, firmware: FirmwareProjection) -> Self {
        Self {
            http,
            firmware: Some(firmware),
        }
    }
}

#[repr(C)]
pub struct PluginConnectionResult {
    pub status: i32,
    pub http_code: u32,
    pub connected: i32,
    pub changed: i32,
    pub auth_rejected: i32,
    pub auth_changed: i32,
    pub transition_ticket: u64,
    pub auth_ticket: u64,
}
