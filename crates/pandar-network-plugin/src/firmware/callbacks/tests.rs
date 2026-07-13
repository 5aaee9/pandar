use std::{ffi::c_void, thread, time::Duration};

use crate::firmware::{
    callbacks::test_hook, pandar_plugin_firmware_next_callback,
    pandar_plugin_firmware_session_create, pandar_plugin_firmware_session_destroy,
    pandar_plugin_firmware_stop,
};

#[test]
fn firmware_ffi_stop_wakes_callback_after_real_wait_entry() {
    let hub_url = b"http://127.0.0.1:9";
    let token = b"token";
    let session = unsafe {
        pandar_plugin_firmware_session_create(
            hub_url.as_ptr(),
            hub_url.len(),
            token.as_ptr(),
            token.len(),
            1,
        )
    };
    assert!(!session.is_null());
    test_hook::arm_callback_wait();
    let address = session as usize;
    let waiter = thread::spawn(move || unsafe {
        let callback = pandar_plugin_firmware_next_callback(address as *mut c_void, 30_000);
        (
            callback.status,
            callback.dev_id_ptr as usize,
            callback.message_ptr as usize,
        )
    });

    assert!(
        test_hook::wait_until_callback_wait_entered(Duration::from_secs(1)),
        "real exported next_callback never entered the callback wait"
    );
    unsafe { pandar_plugin_firmware_stop(session) };
    let (status, dev_id_ptr, message_ptr) = waiter.join().unwrap();
    assert_eq!(status, 1);
    assert_eq!(dev_id_ptr, 0);
    assert_eq!(message_ptr, 0);

    unsafe { pandar_plugin_firmware_session_destroy(session) };
}
