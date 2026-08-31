use super::*;

pub(crate) unsafe fn session(session: *mut c_void) -> Option<&'static ConnectionSession> {
    unsafe { session.cast::<ConnectionSession>().as_ref() }
}

fn empty_connection_result() -> PluginConnectionResult {
    PluginConnectionResult {
        status: 1,
        http_code: 0,
        connected: 0,
        changed: 0,
        auth_rejected: 0,
        auth_changed: 0,
        transition_ticket: 0,
        auth_ticket: 0,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Both byte pointers must be valid for their paired lengths. A non-null return is one caller-owned
/// session that must be destroyed exactly once with `pandar_plugin_printer_refresh_session_destroy`.
pub unsafe extern "C" fn pandar_plugin_printer_refresh_session_create(
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
    Box::into_raw(Box::new(ConnectionSession::new(hub_url, token))).cast()
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_printer_refresh_session_update(
    session_ptr: *mut c_void,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    let Some(hub_url) = unsafe { read_utf8(hub_url_ptr, hub_url_len) }.and_then(normalize_hub_url)
    else {
        return 1;
    };
    let Some(token) = (unsafe { read_utf8(token_ptr, token_len) }) else {
        return 1;
    };
    session.update(hub_url, token);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// `session_ptr` must identify a live connection session. When `callback` is present, it and
/// `context` must remain valid and safe to invoke from the stream worker thread until replaced,
/// cleared with `None`, or the session is destroyed.
pub unsafe extern "C" fn pandar_plugin_connection_set_dispatch_waker(
    session_ptr: *mut c_void,
    context: *mut c_void,
    callback: Option<extern "C" fn(*mut c_void)>,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    session.set_dispatcher_waker(context, callback);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_set_account_epoch(
    session_ptr: *mut c_void,
    account_epoch: u64,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    session.set_account_epoch(account_epoch);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_refresh(
    session_ptr: *mut c_void,
) -> PluginConnectionResult {
    unsafe { session(session_ptr) }.map_or_else(
        empty_connection_result,
        ConnectionSession::refresh_connection,
    )
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_is_connected(session_ptr: *mut c_void) -> i32 {
    unsafe { session(session_ptr) }.is_some_and(ConnectionSession::is_connected) as i32
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_take_transition(
    session_ptr: *mut c_void,
) -> PluginConnectionResult {
    unsafe { session(session_ptr) }
        .map_or_else(empty_connection_result, ConnectionSession::take_transition)
}

/// Stores the account tenant used to build the printer-events stream URL.
#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_printer_refresh_session_set_tenant(
    session_ptr: *mut c_void,
    tenant_id_ptr: *const u8,
    tenant_id_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    match unsafe { read_utf8(tenant_id_ptr, tenant_id_len) } {
        Some(tenant_id) => {
            session.set_tenant(tenant_id);
            0
        }
        None => 1,
    }
}

/// Polls the once-per-episode stream-unavailable notification. Returns
/// `status == 0` with an empty body when nothing is pending, otherwise a
/// `PluginHttpResult` with `http_code == 503` and body
/// `{"error":"printer_event_stream_unavailable"}`.
#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_take_stream_error(
    session_ptr: *mut c_void,
) -> PluginHttpResult {
    match unsafe { session(session_ptr) } {
        Some(session) => session.take_stream_error(),
        None => result(0, 0, String::new()),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_visit_printers(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<ConnectionPrinterVisitor>,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    let Some(printers) = session.fresh_printers() else {
        return 1;
    };
    if let Some(visitor) = visitor {
        for printer in printers {
            visitor(
                context,
                printer.dev_id.as_ptr(),
                printer.dev_id.len(),
                printer.pandar_printer_id.as_ptr(),
                printer.pandar_printer_id.len(),
                printer.model.as_deref().unwrap_or_default().as_ptr(),
                printer.model.as_deref().unwrap_or_default().len(),
                printer.status_report.as_ptr(),
                printer.status_report.len(),
                i32::from(printer.online),
            );
        }
    }
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_printer_eligible(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 0;
    };
    unsafe { read_utf8(dev_id_ptr, dev_id_len) }
        .is_some_and(|dev_id| session.printer_eligible(&dev_id)) as i32
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_take_offline(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<ConnectionDeviceVisitor>,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 1;
    };
    if let Some(visitor) = visitor {
        for dev_id in session.take_offline() {
            visitor(
                context,
                dev_id.dev_id.as_ptr(),
                dev_id.dev_id.len(),
                dev_id.ticket,
            );
        }
    }
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_connection_claim_delivery(
    session_ptr: *mut c_void,
    ticket: u64,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    i32::from(session.claim_delivery(ticket))
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be null or a uniquely owned handle returned by
/// `pandar_plugin_printer_refresh_session_create`. It must not be borrowed or used after this call.
pub unsafe extern "C" fn pandar_plugin_printer_refresh_session_destroy(session: *mut c_void) {
    if !session.is_null() {
        let boxed = unsafe { Box::from_raw(session.cast::<ConnectionSession>()) };
        boxed.stop_worker();
        drop(boxed);
    }
}
