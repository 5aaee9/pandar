use super::*;

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_startup_uses_existing_pushall_contract() {
    let transport = ControlledTransport::new(None);
    let (task, _receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    let published = transport.published_commands();
    assert_eq!(published.len(), 1);
    let (ordinal, command) = &published[0];
    assert_eq!(*ordinal, 1);
    assert_eq!(command.topic, "device/01S00EXAMPLE/request");
    assert_eq!(command.qos, 1);
    let sequence_id = pushall_sequence_id(command);
    assert_eq!(
        command.payload,
        json!({
            "pushing": {
                "command": "pushall",
                "sequence_id": sequence_id,
                "version": 1,
                "push_target": 1
            }
        })
    );
    let operations = transport.operations();
    assert!(matches!(
        &operations[0],
        ControlledOperation::Subscribe(topic) if topic == "device/01S00EXAMPLE/report"
    ));
    assert!(matches!(
        &operations[1],
        ControlledOperation::Publish { ordinal: 1, .. }
    ));
    assert!(matches!(
        &operations[2],
        ControlledOperation::ReportWaitArmed(1)
    ));

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_publishes_at_exact_sixty_second_deadline() {
    let transport = ControlledTransport::new(None);
    let (task, _receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(EXPECTED_REFRESH_INTERVAL - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 1);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_publish_attempts(2).await;
    assert_eq!(transport.publish_attempts(), 2);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_unsolicited_report_does_not_move_deadline() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(Duration::from_secs(30)).await;
    transport.push_report(json!({ "print": { "bed_temper": 42.0 } }));
    transport.wait_for_report_waits(2).await;
    let mut saw_snapshot = false;
    while let Ok(event) = receiver.try_recv() {
        saw_snapshot |= matches!(event.event, Some(agent_event::Event::PrinterSnapshot(_)));
    }
    assert!(
        saw_snapshot,
        "qualifying report must be forwarded immediately"
    );

    advance(Duration::from_secs(30) - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 1);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_publish_attempts(2).await;

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn chamber_target_only_report_keeps_absent_snapshot_fields_absent() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    transport.push_report(json!({
        "print": { "command": "push_status", "ctt": 48 }
    }));
    transport.wait_for_report_waits(2).await;
    let snapshot = loop {
        let event = next_event(&mut receiver).await;
        if let Some(agent_event::Event::PrinterSnapshot(snapshot)) = event.event {
            break snapshot;
        }
    };

    assert_eq!(snapshot.chamber_target_temperature_celsius, "48");
    assert!(
        !snapshot.telemetry_authoritative,
        "unsolicited MQTT reports are partial telemetry patches"
    );
    assert!(
        snapshot.state.is_empty(),
        "an absent printer state must not be synthesized as a present `unknown` value"
    );
    assert!(snapshot.nozzle_temperatures.is_empty());
    assert!(snapshot.active_nozzle.is_empty());
    assert!(snapshot.bed_temperature_celsius.is_empty());
    assert!(snapshot.bed_target_temperature_celsius.is_empty());
    assert!(snapshot.chamber_temperature_celsius.is_empty());
    assert_eq!(snapshot.chamber_light_on, None);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_missed_ticks_delay_without_burst() {
    let transport = ControlledTransport::new(None);
    let (task, _receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(Duration::from_secs(180)).await;
    transport.wait_for_publish_attempts(2).await;
    transport.wait_for_report_waits(2).await;
    assert_eq!(transport.publish_attempts(), 2);
    advance(EXPECTED_REFRESH_INTERVAL - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 2);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_publish_attempts(3).await;
    assert_eq!(transport.publish_attempts(), 3);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_sender_closure_wins_when_deadline_is_due() {
    let transport = ControlledTransport::new(None);
    let (task, receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    drop(receiver);
    advance(EXPECTED_REFRESH_INTERVAL).await;
    wait_for_task_finish(&task).await;
    task.await.unwrap().unwrap();
    assert_eq!(transport.publish_attempts(), 1);
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_due_tick_wins_over_ready_reports() {
    let transport = ControlledTransport::new(None);
    let (task, _receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    transport.make_reports_ready_without_waking(32);
    advance(EXPECTED_REFRESH_INTERVAL).await;
    transport.wait_for_publish_attempts(2).await;
    transport.wait_for_deliveries(1).await;

    let operations = transport.operations();
    let publish_index = operations
        .iter()
        .position(|operation| matches!(operation, ControlledOperation::Publish { ordinal: 2, .. }))
        .expect("periodic publish is recorded");
    let delivery_index = operations
        .iter()
        .position(|operation| matches!(operation, ControlledOperation::ReportDelivered(1)))
        .expect("ready report delivery is recorded");
    assert!(publish_index < delivery_index);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_without_response_emits_no_presence_event() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(EXPECTED_REFRESH_INTERVAL).await;
    transport.wait_for_publish_attempts(2).await;
    assert!(receiver.try_recv().is_err());

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_receiver_close_stops_future_requests() {
    let transport = ControlledTransport::new(None);
    let (task, receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(Duration::from_secs(30)).await;
    drop(receiver);
    wait_for_task_finish(&task).await;
    task.await.unwrap().unwrap();
    advance(Duration::from_secs(120)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 1);
}

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_publish_failure_preserves_context_chain() {
    let transport = ControlledTransport::new(Some(2));
    let (task, _receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    advance(EXPECTED_REFRESH_INTERVAL).await;
    wait_for_task_finish(&task).await;
    let error = task.await.unwrap().unwrap_err();
    let chain = format!("{error:#}");
    assert!(
        chain.contains("publish periodic pushall to request topic device/01S00EXAMPLE/request"),
        "missing periodic topic context: {chain}"
    );
    assert!(
        chain.contains("controlled MQTT publish failure at attempt 2"),
        "missing lower transport cause: {chain}"
    );
}
