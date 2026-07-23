use super::*;

#[test]
fn selected_cloud_machine_is_a_status_target_without_explicit_subscription() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let online = INITIAL_PRINTERS_RESPONSE.replace(
        r#""dev_online":false,"online":false"#,
        r#""dev_online":true,"online":true"#,
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            online.len(),
            online
        )
        .unwrap();
    });
    let session = create_session(&hub_url, "token");
    set_listener(session, CLOUD_MESSAGE_LISTENER);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(super::super::refresh_without_observation(session).status, 0);

    assert_eq!(
        pandar_plugin_studio_status_target_available(
            session,
            CLOUD_TUNNEL,
            b"serial-1".as_ptr(),
            8,
            0,
        ),
        1
    );
    let mut targets = Vec::new();
    let plan = pandar_plugin_studio_heartbeat_plan(
        session,
        (&mut targets as *mut Vec<Target>).cast(),
        Some(copy_target),
    );
    assert_eq!((plan.wait_ms, plan.refresh), (2_000, 1));
    assert_eq!(
        targets,
        vec![Target {
            tunnel: CLOUD_TUNNEL,
            dev_id: "serial-1".to_owned(),
            generation: 0,
        }]
    );

    let mut payload = Payload::default();
    let delivery = pandar_plugin_studio_prepare_message(
        session,
        CLOUD_TUNNEL,
        b"serial-1".as_ptr(),
        8,
        0,
        false,
        0,
        (&mut payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(delivery.status, 0);
    assert_eq!(payload.dev_id, "serial-1");
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, delivery.ticket, true),
        1
    );

    assert_eq!(
        pandar_plugin_studio_add_subscription(session, b"serial-1".as_ptr(), 8),
        0
    );
    let delivery = pandar_plugin_studio_prepare_message(
        session,
        CLOUD_TUNNEL,
        b"serial-1".as_ptr(),
        8,
        0,
        true,
        0,
        (&mut payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(delivery.status, 0);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-2".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, delivery.ticket, true),
        1
    );

    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    let delivery = pandar_plugin_studio_prepare_message(
        session,
        CLOUD_TUNNEL,
        b"serial-1".as_ptr(),
        8,
        0,
        true,
        0,
        (&mut payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(delivery.status, 0);
    assert_eq!(
        pandar_plugin_studio_del_subscription(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, delivery.ticket, true),
        1
    );

    set_listener(session, PRINTER_CONNECTED_LISTENER);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-2".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(prepare_connected(session, 1, &mut payload).status, 0);

    server.join().unwrap();
    super::super::pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn changing_cloud_selection_keeps_pending_local_deliveries() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let online = INITIAL_PRINTERS_RESPONSE.replace(
        r#""dev_online":false,"online":false"#,
        r#""dev_online":true,"online":true"#,
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            online.len(),
            online
        )
        .unwrap();
    });
    let session = create_session(&hub_url, "token");
    set_listener(session, LOCAL_CONNECTED_LISTENER);
    set_listener(session, LOCAL_MESSAGE_LISTENER);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(super::super::refresh_without_observation(session).status, 0);

    let mut payload = Payload::default();
    let local = pandar_plugin_studio_connect_local(
        session,
        b"serial-1".as_ptr(),
        8,
        (&mut payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(local.status, 0);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-2".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, local.ticket, true),
        1
    );

    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    let message = pandar_plugin_studio_prepare_message(
        session,
        LOCAL_TUNNEL,
        b"serial-1".as_ptr(),
        8,
        local.local_generation,
        false,
        0,
        (&mut payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(message.status, 0);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-2".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, message.ticket, true),
        1
    );

    server.join().unwrap();
    super::super::pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn heartbeat_includes_distinct_selected_and_subscribed_cloud_targets() {
    let session = create_session("http://127.0.0.1:9", "token");
    set_listener(session, CLOUD_MESSAGE_LISTENER);
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-a".as_ptr(), 8),
        0
    );
    assert_eq!(
        pandar_plugin_studio_add_subscription(session, b"serial-b".as_ptr(), 8),
        0
    );

    let mut targets = Vec::new();
    let plan = pandar_plugin_studio_heartbeat_plan(
        session,
        (&mut targets as *mut Vec<Target>).cast(),
        Some(copy_target),
    );
    assert_eq!((plan.wait_ms, plan.refresh), (2_000, 1));
    assert_eq!(
        targets,
        vec![
            Target {
                tunnel: CLOUD_TUNNEL,
                dev_id: "serial-a".to_owned(),
                generation: 0,
            },
            Target {
                tunnel: CLOUD_TUNNEL,
                dev_id: "serial-b".to_owned(),
                generation: 0,
            },
        ]
    );

    super::super::pandar_plugin_printer_refresh_session_destroy(session);
}
