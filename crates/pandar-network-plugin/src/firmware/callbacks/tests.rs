use std::{ffi::c_void, thread, time::Duration};

use crate::firmware::{
    callbacks::{FirmwareCallback, FirmwareCallbackQueue, FirmwareTunnel, test_hook},
    pandar_plugin_firmware_next_callback, pandar_plugin_firmware_session_create,
    pandar_plugin_firmware_session_destroy, pandar_plugin_firmware_stop,
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

#[test]
fn dequeued_callback_retains_captured_generation_after_cancellation() {
    let queue = FirmwareCallbackQueue::new();
    let token = queue
        .push(
            7,
            FirmwareCallback {
                dev_id: "SERIAL".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: "callback".into(),
            },
        )
        .unwrap();
    let handoff = std::time::Instant::now();
    assert!(queue.return_handoff_at(token, 1, 2, 3, handoff));

    let ready = queue
        .take_ready_at(handoff + Duration::from_millis(1_100))
        .unwrap();
    queue.cancel_generation(7);
    let _ = queue.push(
        8,
        FirmwareCallback {
            dev_id: "NEXT".into(),
            tunnel: FirmwareTunnel::Cloud,
            message: "next".into(),
        },
    );

    assert_eq!(ready.generation, 7);
}

#[test]
fn cancelling_old_generation_cannot_relabel_its_ready_callback() {
    let queue = FirmwareCallbackQueue::new();
    let old = queue
        .push(
            7,
            FirmwareCallback {
                dev_id: "OLD".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: "old".into(),
            },
        )
        .unwrap();
    let current = queue
        .push(
            8,
            FirmwareCallback {
                dev_id: "CURRENT".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: "current".into(),
            },
        )
        .unwrap();
    let handoff = std::time::Instant::now();
    assert!(queue.return_handoff_at(old, 1, 2, 3, handoff));
    assert!(queue.return_handoff_at(current, 4, 5, 6, handoff));

    queue.cancel_generation(7);
    let ready = queue
        .take_ready_at(handoff + Duration::from_millis(1_100))
        .unwrap();

    assert_eq!(ready.dev_id, "CURRENT");
    assert_eq!(ready.generation, 8);
    assert!(
        queue
            .take_ready_at(handoff + Duration::from_millis(1_100))
            .is_none()
    );
}
