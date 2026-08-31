use std::{
    ffi::c_void,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use super::*;

mod recovered_account;

struct SnapshotState {
    hub_url: String,
    token: String,
    account_epoch: u64,
}

extern "C" fn switched_account_snapshot(
    context: *mut c_void,
    snapshot: *mut PluginStudioSnapshot,
) -> i32 {
    let state = unsafe { &*(context.cast::<SnapshotState>()) };
    unsafe {
        *snapshot = PluginStudioSnapshot {
            hub_url: bytes(&state.hub_url),
            token: bytes(&state.token),
            printer_id: bytes(""),
            printer_authorized: 0,
            account_transition_pending: 0,
            account_epoch: state.account_epoch,
            cache_generation: 0,
            firmware_generation: 0,
        };
    }
    1
}

#[test]
fn stale_finished_follower_returns_the_task_stale_response() {
    let hub_url = "http://hub";
    let token = "old-a-token";
    let account = PluginStudioAccount {
        snapshot: PluginStudioSnapshot {
            hub_url: bytes(hub_url),
            token: bytes(token),
            printer_id: bytes(""),
            printer_authorized: 0,
            account_transition_pending: 0,
            account_epoch: 7,
            cache_generation: 0,
            firmware_generation: 0,
        },
        context: std::ptr::null_mut(),
        current_snapshot: None,
    };

    let Recovery::Failure(result) = finish_recovery(&account, NoAuthRecovery::Stale) else {
        panic!("stale follower did not fail the task request");
    };
    let outcome = take_http(result);
    assert_eq!(outcome.status, 1);
    assert_eq!(outcome.http_code, 409);
    assert_eq!(outcome.body, r#"{"error":"stale_task_response"}"#);
}
