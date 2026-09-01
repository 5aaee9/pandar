use super::*;
use crate::pandar_plugin_free_with_capacity;

fn body(result: PluginHttpResult) -> (i32, u32, String) {
    let bytes = if result.body_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) }
    };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    unsafe {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap)
    };
    (result.status, result.http_code, body)
}

#[test]
fn print_info_policy_owns_admission_and_stale_response() {
    assert_eq!(
        body(pandar_plugin_studio_request_admitted(true, false)),
        (0, 0, String::new())
    );
    assert_eq!(
        body(pandar_plugin_studio_request_admitted(false, false)),
        (-19, 404, r#"{"error":"invalid_printer_id"}"#.to_owned())
    );
    assert_eq!(
        body(pandar_plugin_studio_request_admitted(false, true)),
        (-19, 409, r#"{"error":"account_transition"}"#.to_owned())
    );
    assert_eq!(
        body(unsafe {
            pandar_plugin_studio_print_info_admission(true, true, b"token".as_ptr(), 5)
        }),
        (-11, 409, r#"{"error":"account_transition"}"#.to_owned())
    );
    assert_eq!(
        body(unsafe { pandar_plugin_studio_print_info_result(0, 200, b"[]".as_ptr(), 2, false,) }),
        (-11, 401, r#"{"error":"invalid_auth_token"}"#.to_owned())
    );
}

#[test]
fn printer_response_policy_owns_stale_dispositions() {
    assert_eq!(
        body(pandar_plugin_studio_file_transfer_unavailable()),
        (
            ABI_INVALID_RESULT,
            501,
            r#"{"error":"unsupported_file_transfer"}"#.to_owned()
        )
    );
    assert_eq!(
        body(unsafe {
            pandar_plugin_studio_firmware_catalog_result(0, 200, b"{}".as_ptr(), 2, false)
        }),
        (-19, 409, r#"{"error":"stale_firmware_catalog"}"#.to_owned())
    );
    assert_eq!(
        body(unsafe {
            pandar_plugin_studio_printer_operation_result(0, 200, b"{}".as_ptr(), 2, false)
        }),
        (
            -19,
            409,
            r#"{"error":"stale_printer_operation"}"#.to_owned()
        )
    );
}

#[test]
fn account_policy_owns_state_changes_and_abi_status() {
    assert_eq!(
        unsafe {
            login_observation::pandar_plugin_account_logout_action(1, false, 0, b"".as_ptr(), 0)
        },
        ACCOUNT_ACTION_NONE
    );
    assert_eq!(
        unsafe {
            login_observation::pandar_plugin_account_logout_action(1, true, 0, b"".as_ptr(), 0)
        },
        ACCOUNT_ACTION_LOGOUT
    );
    assert_eq!(
        unsafe {
            login_observation::pandar_plugin_account_logout_action(1, true, 0, b"".as_ptr(), 0)
        },
        ACCOUNT_ACTION_LOGOUT
    );
    assert_eq!(
        unsafe {
            login_observation::pandar_plugin_account_logout_action(
                1,
                false,
                0,
                b"token".as_ptr(),
                5,
            )
        },
        ACCOUNT_ACTION_LOGOUT
    );
    assert_eq!(
        unsafe {
            login_observation::pandar_plugin_account_logout_action(1, true, 0, [0xff].as_ptr(), 1)
        },
        ACCOUNT_ACTION_FAILURE
    );
    assert_eq!(
        unsafe {
            pandar_plugin_account_commit_action(7, 8, b"".as_ptr(), 0, b"".as_ptr(), 0, true)
        },
        ACCOUNT_ACTION_NONE
    );
    assert_eq!(
        unsafe {
            pandar_plugin_account_commit_action(7, 7, b"".as_ptr(), 0, b"token".as_ptr(), 5, true)
        },
        ACCOUNT_ACTION_NONE
    );
    assert_eq!(
        unsafe {
            pandar_plugin_account_commit_action(7, 7, b"".as_ptr(), 0, b"".as_ptr(), 0, true)
        },
        ACCOUNT_ACTION_APPLY
    );
    assert_eq!(
        unsafe {
            pandar_plugin_account_commit_action(7, 7, b"old".as_ptr(), 3, b"new".as_ptr(), 3, false)
        },
        ACCOUNT_ACTION_NONE
    );
    let refresh = |current_epoch, current_config_epoch, pending, current_token: &[u8]| unsafe {
        pandar_plugin_account_refresh_action(
            7,
            current_epoch,
            11,
            current_config_epoch,
            pending,
            2,
            2,
            b"http://hub".as_ptr(),
            10,
            b"http://hub".as_ptr(),
            10,
            b"old".as_ptr(),
            3,
            current_token.as_ptr(),
            current_token.len(),
        )
    };
    assert_eq!(refresh(7, 11, false, b"old"), ACCOUNT_ACTION_APPLY);
    assert_eq!(refresh(7, 11, false, b"new"), ACCOUNT_ACTION_LOGIN);
    assert_eq!(refresh(8, 11, false, b"new"), ACCOUNT_ACTION_NONE);
    assert_eq!(refresh(7, 12, false, b"new"), ACCOUNT_ACTION_NONE);
    assert_eq!(refresh(7, 11, true, b"new"), ACCOUNT_ACTION_NONE);
    assert_eq!(refresh(7, 11, false, b""), ACCOUNT_ACTION_NONE);
}

#[test]
fn studio_info_url_policy_owns_configured_and_unavailable_results() {
    assert_eq!(
        body(unsafe {
            pandar_plugin_account_studio_info_url(true, true, b"http://studio".as_ptr(), 13)
        }),
        (0, 200, "http://studio".to_owned())
    );
    let unavailable =
        body(unsafe { pandar_plugin_account_studio_info_url(true, false, b"".as_ptr(), 0) });
    assert_eq!(unavailable.0, -19);
    assert_eq!(unavailable.1, 501);
    assert!(unavailable.2.contains("studio_info_url_unconfigured"));
}
