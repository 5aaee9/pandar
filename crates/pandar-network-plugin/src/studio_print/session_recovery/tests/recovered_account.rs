use super::*;

#[test]
fn recovered_task_retry_keeps_the_recovered_account_when_current_account_switched() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind task retry server");
    let hub_url = format!(
        "http://{}",
        listener.local_addr().expect("task retry address")
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept task retry request");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).expect("read task retry request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let body = r#"{"total":0,"hits":[]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write task retry response");
        String::from_utf8(request).expect("task retry request is UTF-8")
    });

    let recovered_token = "recovered-a-token";
    let mut current = SnapshotState {
        hub_url: hub_url.clone(),
        token: "account-b-token".to_owned(),
        account_epoch: 8,
    };
    let recovered_snapshot = PluginStudioSnapshot {
        hub_url: bytes(&hub_url),
        token: bytes(recovered_token),
        printer_id: bytes(""),
        printer_authorized: 0,
        account_transition_pending: 0,
        account_epoch: 7,
        cache_generation: 0,
        firmware_generation: 0,
    };
    let account = PluginStudioAccount {
        snapshot: recovered_snapshot,
        context: (&mut current as *mut SnapshotState).cast(),
        current_snapshot: Some(switched_account_snapshot),
    };
    let retry = BoundRetryAccount::new(
        NoAuthExpected {
            hub_url: hub_url.clone(),
            token: recovered_token.to_owned(),
            account_epoch: 7,
            config_epoch: 3,
            session_kind: 2,
        },
        &account,
    );
    let query = PluginStudioTaskQuery {
        dev_id: bytes(""),
        status: 0,
        offset: 0,
        limit: 20,
    };

    let result =
        retry.with_account(|account| unsafe { pandar_plugin_studio_get_tasks(account, &query) });
    let outcome = take_http(result);
    let request = server.join().expect("task retry server");

    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer recovered-a-token"),
        "retry crossed accounts: {request}"
    );
    assert_eq!(outcome.status, 1);
    assert_eq!(outcome.http_code, 409);
    assert_eq!(outcome.body, r#"{"error":"stale_task_response"}"#);
}

extern "C" fn count_model_task(
    context: *mut c_void,
    _: *const crate::studio_print::PluginStudioModelTask,
) -> i32 {
    unsafe {
        *context.cast::<usize>() += 1;
    }
    1
}

#[test]
fn recovered_model_task_does_not_publish_after_account_switch() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model-task retry server");
    let hub_url = format!(
        "http://{}",
        listener.local_addr().expect("model-task retry address")
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model-task retry request");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream
                .read(&mut chunk)
                .expect("read model-task retry request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let body = r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write model-task retry response");
        String::from_utf8(request).expect("model-task retry request is UTF-8")
    });

    let recovered_token = "recovered-a-token";
    let mut current = SnapshotState {
        hub_url: hub_url.clone(),
        token: "account-b-token".to_owned(),
        account_epoch: 8,
    };
    let account = PluginStudioAccount {
        snapshot: PluginStudioSnapshot {
            hub_url: bytes(&hub_url),
            token: bytes(recovered_token),
            printer_id: bytes(""),
            printer_authorized: 0,
            account_transition_pending: 0,
            account_epoch: 7,
            cache_generation: 0,
            firmware_generation: 0,
        },
        context: (&mut current as *mut SnapshotState).cast(),
        current_snapshot: Some(switched_account_snapshot),
    };
    let retry = BoundRetryAccount::new(
        NoAuthExpected {
            hub_url: hub_url.clone(),
            token: recovered_token.to_owned(),
            account_epoch: 7,
            config_epoch: 3,
            session_kind: 2,
        },
        &account,
    );
    let mut deliveries = 0_usize;

    let result = retry.with_account(|account| {
        pandar_plugin_studio_get_model_task(
            account,
            bytes("41"),
            (&mut deliveries as *mut usize).cast(),
            Some(count_model_task),
        )
    });
    let outcome = take_http(result);
    let request = server.join().expect("model-task retry server");

    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer recovered-a-token")
    );
    assert_eq!(outcome.status, 1);
    assert_eq!(outcome.http_code, 409);
    assert_eq!(outcome.body, r#"{"error":"stale_task_response"}"#);
    assert_eq!(deliveries, 0);
}
