use super::*;

#[test]
fn connection_answers_hub_ping_without_reconnecting() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_A_ONLINE]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);

    let first = Arc::clone(&requests.lock().unwrap()[0]);
    first.frames.send("@ping".to_owned()).unwrap();
    wait_until(|| first.pongs.load(Ordering::Relaxed) == 1);
    let (_, _, envelope) = cached_print_info(session);
    assert!(envelope.contains("serial-1"), "{envelope}");
    assert_eq!(requests.lock().unwrap().len(), 1, "no reconnect happened");
    let plan = unsafe {
        pandar_plugin_studio_heartbeat_plan(
            session,
            std::ptr::null_mut(),
            Some(noop_heartbeat as StudioHeartbeatVisitor),
        )
    };
    assert_eq!(plan.refresh, 0);
    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn malformed_or_unsupported_initial_stream_never_commits_staged_records() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(
            format!(
                "{{\"type\":\"snapshot_begin\",\"version\":1}}\n{}\n{{\"type\":\"future_frame\"}}\n@close",
                upsert_frame(DEVICE_A_ONLINE)
            ),
        )))
        .unwrap();
    wait_until(|| !requests.lock().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        cached_print_info(session).0,
        1,
        "malformed snapshot committed"
    );

    scripts
        .send(Some(Script::Frames(
            "{\"type\":\"snapshot_begin\",\"version\":2}\n@close".to_owned(),
        )))
        .unwrap();
    wait_until(|| requests.lock().unwrap().len() >= 2);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        cached_print_info(session).0,
        1,
        "version two snapshot committed"
    );

    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_B_ONLINE]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);
    let (_, _, envelope) = cached_print_info(session);
    assert!(envelope.contains("serial-2"), "{envelope}");
    assert!(!envelope.contains("serial-1"), "{envelope}");
    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}

#[test]
fn established_stream_close_waits_one_second_before_redial() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    let mut first = snapshot_script(&[DEVICE_A_ONLINE]);
    first.push_str("@close\n");
    scripts.send(Some(Script::Frames(first))).unwrap();
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_A_ONLINE]))))
        .unwrap();
    wait_until(|| requests.lock().unwrap().len() >= 2);
    let requests = requests.lock().unwrap();
    let elapsed = requests[1]
        .accepted_at
        .duration_since(requests[0].accepted_at);
    assert!(
        elapsed >= Duration::from_millis(900),
        "redial skipped first backoff: {elapsed:?}"
    );
    drop(requests);
    unsafe { pandar_plugin_printer_refresh_session_destroy(session) };
}
