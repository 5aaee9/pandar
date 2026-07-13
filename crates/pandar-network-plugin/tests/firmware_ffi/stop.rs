use std::thread;

use super::abi::{Session, next_callback};

#[test]
fn firmware_ffi_stopped_session_rejects_callback_before_destroy() {
    let session = Session::create("http://127.0.0.1:9", "token", 1);
    let address = session.address();
    session.stop();
    let waiter = thread::spawn(move || next_callback(address, 30_000));
    let callback = waiter.join().unwrap();
    assert_eq!(callback.status, 1);
    assert!(callback.dev_id.is_empty());
    assert!(callback.message.is_empty());

    session.destroy();
}
