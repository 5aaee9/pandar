use std::ffi::c_void;

pub type PrinterRefreshObservationReservation = extern "C" fn(*mut c_void);
pub type PrinterRefreshTransaction = unsafe extern "C" fn(*mut c_void) -> i32;
pub type PrinterRefreshWithLock =
    unsafe extern "C" fn(*mut c_void, *mut c_void, Option<PrinterRefreshTransaction>) -> i32;
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
    pub collect_offline: Option<ConnectionDeviceVisitor>,
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
