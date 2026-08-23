use super::*;

#[test]
fn snapshot_commit_serves_cache_and_validates_the_upgrade_request() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_A_ONLINE]))))
        .unwrap();

    wait_until(|| !requests.lock().unwrap().is_empty());
    let first = Arc::clone(&requests.lock().unwrap()[0]);
    assert_eq!(
        first.request_line,
        "/api/v1/tenants/tenant-1/printer-events?projection=studio&version=1"
    );
    assert_eq!(first.authorization.as_deref(), Some("Bearer stream-token"));

    wait_until(|| cached_print_info(session).0 == 0);
    let (status, code, envelope) = cached_print_info(session);
    assert_eq!((status, code), (0, 200));
    assert_eq!(
        envelope,
        format!("{{\"message\":\"success\",\"devices\":[{DEVICE_A_ONLINE}]}}")
    );
    let printers = Mutex::new(Vec::<String>::new());
    assert_eq!(
        0,
        pandar_plugin_connection_visit_printers(
            session,
            &printers as *const _ as *mut c_void,
            Some(collect_printer),
        )
    );
    assert_eq!(*printers.lock().unwrap(), vec!["serial-1".to_owned()]);
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn post_snapshot_upsert_queues_latest_wins_cloud_status_for_selected_target_only() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(snapshot_script(&[
            DEVICE_A_ONLINE,
            DEVICE_B_ONLINE,
        ]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);

    // Only serial-1 is selected and the cloud listener registered, so only
    // its upserts schedule deliveries.
    assert_eq!(
        0,
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), "serial-1".len())
    );
    assert_eq!(
        0,
        pandar_plugin_studio_set_listener(session, 1 /* cloud message */, true)
    );
    let plan_a = pandar_plugin_studio_heartbeat_plan(
        session,
        std::ptr::null_mut(),
        None::<StudioHeartbeatVisitor>,
    );
    assert_eq!(
        plan_a.refresh, 0,
        "polling is dead: plan never asks for refresh"
    );
    assert_eq!(plan_a.wait_ms, u32::MAX);
    let work = Mutex::new(Vec::<(i32, String, String)>::new());
    pandar_plugin_studio_take_work(
        session,
        &work as *const _ as *mut c_void,
        Some(collect_work as StudioWorkVisitor),
    );
    work.lock().unwrap().clear();

    let frames = requests.lock().unwrap()[0].frames.clone();
    frames.send(upsert_frame(DEVICE_A_ONLINE)).unwrap();
    let device_a_late = DEVICE_A_ONLINE.replace("\"mc_percent\":7", "\"mc_percent\":99");
    frames.send(upsert_frame(&device_a_late)).unwrap();
    frames.send(upsert_frame(DEVICE_B_ONLINE)).unwrap();

    wait_until(|| {
        pandar_plugin_studio_take_work(
            session,
            &work as *const _ as *mut c_void,
            Some(collect_work as StudioWorkVisitor),
        );
        !work.lock().unwrap().is_empty()
    });
    let status_work: Vec<_> = work
        .lock()
        .unwrap()
        .iter()
        .filter(|item| item.0 == 1)
        .cloned()
        .collect();
    assert_eq!(
        status_work.len(),
        1,
        "latest-wins coalescing: {status_work:?}"
    );
    assert_eq!(status_work[0].1, "serial-1");
    assert!(status_work[0].2.contains("\"mc_percent\":99"));

    let plan_b = pandar_plugin_studio_heartbeat_plan(
        session,
        std::ptr::null_mut(),
        None::<StudioHeartbeatVisitor>,
    );
    assert_eq!(plan_b.wait_ms, 0, "queued work wakes the dispatcher");
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn printer_removed_is_a_once_only_offline_transition() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_A_ONLINE]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);

    let frames = requests.lock().unwrap()[0].frames.clone();
    frames.send(removed_frame("serial-1")).unwrap();
    frames.send(removed_frame("serial-1")).unwrap();

    let offline = Mutex::new(Vec::<String>::new());
    wait_until(|| {
        pandar_plugin_connection_take_offline(
            session,
            &offline as *const _ as *mut c_void,
            Some(collect_offline),
        );
        !offline.lock().unwrap().is_empty()
    });
    std::thread::sleep(Duration::from_millis(150));
    pandar_plugin_connection_take_offline(
        session,
        &offline as *const _ as *mut c_void,
        Some(collect_offline),
    );
    assert_eq!(*offline.lock().unwrap(), vec!["serial-1".to_owned()]);
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn incomplete_snapshot_is_discarded_until_a_complete_one_commits() {
    let (addr, _requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    // First connection stages records but never finishes the snapshot.
    scripts
        .send(Some(Script::Frames(format!(
            "{{\"type\":\"snapshot_begin\",\"version\":1}}\n{}\n@close",
            upsert_frame(DEVICE_A_ONLINE)
        ))))
        .unwrap();
    let (status, code, _) = cached_print_info(session);
    assert_eq!((status, code), (1, 503), "staged records stay uncommitted");

    // Second connection delivers a complete snapshot.
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_B_ONLINE]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);
    let (_, _, envelope) = cached_print_info(session);
    assert!(envelope.contains("serial-2"), "{envelope}");
    assert!(!envelope.contains("serial-1"), "{envelope}");
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn upgrade_auth_rejection_rejects_auth_once_without_connectivity_loss() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts.send(Some(Script::Reject401)).unwrap();

    wait_until(|| !requests.lock().unwrap().is_empty());
    let mut saw_rejection = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let result: PluginConnectionResult = pandar_plugin_connection_take_transition(session);
        if result.auth_changed != 0 {
            assert_ne!(result.auth_ticket, 0);
            assert_eq!(result.auth_rejected, 1);
            saw_rejection = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(saw_rejection, "auth rejection was never surfaced");

    // The hub answered, so connectivity must not report -2.
    let result = pandar_plugin_connection_take_transition(session);
    assert_eq!(result.transition_ticket, 0);
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn account_epoch_change_fences_frames_from_the_previous_generation() {
    let (addr, requests, scripts) = spawn_hub();
    let session = create_session(&format!("http://{addr}"));
    set_tenant(session, "tenant-1");
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_A_ONLINE]))))
        .unwrap();
    wait_until(|| cached_print_info(session).0 == 0);

    // Bumping the account epoch fences every in-flight frame; the next frame
    // on the old socket is refused, the worker reconnects, and the queued
    // fresh snapshot repopulates the cache under the new generation.
    assert_eq!(0, pandar_plugin_connection_set_account_epoch(session, 9));
    scripts
        .send(Some(Script::Frames(snapshot_script(&[DEVICE_B_ONLINE]))))
        .unwrap();
    let frames = requests.lock().unwrap()[0].frames.clone();
    frames.send(upsert_frame(DEVICE_A_ONLINE)).unwrap();

    wait_until(|| cached_print_info(session).0 == 0);
    let (_, _, envelope) = cached_print_info(session);
    assert!(envelope.contains("serial-2"), "{envelope}");
    pandar_plugin_printer_refresh_session_destroy(session);
}
