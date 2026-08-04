use super::*;

#[test]
fn periodic_printer_refresh_uses_exact_sixty_second_constant() {
    assert_eq!(
        super::super::PRINTER_REFRESH_INTERVAL,
        Duration::from_secs(60)
    );
}

#[tokio::test(start_paused = true)]
async fn initial_pushall_full_report_is_authoritative_even_when_telemetry_is_empty() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;
    let sequence_id = pushall_sequence_id(&transport.published_commands()[0].1).to_owned();

    transport.push_report(json!({
        "print": { "command": "push_status", "msg": 0, "sequence_id": sequence_id }
    }));
    transport.wait_for_report_waits(2).await;

    let snapshot = next_snapshot(&mut receiver).await;
    assert!(snapshot.telemetry_authoritative);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn full_push_status_authority_ignores_sequence_and_request_correlation() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;
    let sequence_id = pushall_sequence_id(&transport.published_commands()[0].1).to_owned();

    for report in [
        json!({ "print": { "command": "push_status", "msg": 1, "sequence_id": sequence_id, "ctt": 41 } }),
        json!({ "print": { "command": "get_version", "msg": 0, "sequence_id": sequence_id, "ctt": 42 } }),
    ] {
        transport.push_report(report);
        let snapshot = next_snapshot(&mut receiver).await;
        assert!(!snapshot.telemetry_authoritative);
    }

    for report in [
        json!({ "print": { "command": "push_status", "msg": 0, "ctt": 43 } }),
        json!({ "print": { "command": "push_status", "msg": 0, "sequence_id": "printer-owned", "ctt": 44 } }),
        json!({ "print": { "command": "push_status", "msg": 0, "sequence_id": sequence_id, "ctt": 45 } }),
    ] {
        transport.push_report(report);
        let snapshot = next_snapshot(&mut receiver).await;
        assert!(snapshot.telemetry_authoritative);
    }

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn periodic_pushall_sequence_does_not_gate_full_snapshot_authority() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;
    let startup_sequence = pushall_sequence_id(&transport.published_commands()[0].1).to_owned();

    advance(EXPECTED_REFRESH_INTERVAL).await;
    transport.wait_for_publish_attempts(2).await;
    let periodic_sequence = pushall_sequence_id(&transport.published_commands()[1].1).to_owned();
    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": startup_sequence,
            "gcode_state": "STALE"
        }
    }));
    let startup_report = next_snapshot(&mut receiver).await;
    assert!(startup_report.telemetry_authoritative);
    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": periodic_sequence,
            "gcode_state": "IDLE"
        }
    }));

    let snapshot = next_snapshot(&mut receiver).await;
    assert!(snapshot.telemetry_authoritative);
    assert_eq!(snapshot.state, "IDLE");

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn state_only_partial_report_is_forwarded_without_overwriting_model() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    transport.push_report(json!({
        "print": { "command": "push_status", "msg": 1, "gcode_state": "RUNNING" }
    }));

    let snapshot = next_snapshot(&mut receiver).await;
    assert_eq!(snapshot.state, "RUNNING");
    assert!(snapshot.model.is_empty());
    assert!(!snapshot.telemetry_authoritative);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn state_only_full_report_updates_live_and_snapshot_state() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;
    let sequence_id = pushall_sequence_id(&transport.published_commands()[0].1).to_owned();

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": sequence_id,
            "state": "IDLE"
        }
    }));

    let live = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrintJobReport(live)) = live.event else {
        panic!("state-only full report must update live print state first");
    };
    assert_eq!(live.gcode_state, "IDLE");

    let snapshot = next_snapshot(&mut receiver).await;
    assert_eq!(snapshot.state, "IDLE");
    assert!(snapshot.telemetry_authoritative);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn idle_timeout_emits_offline_once_and_any_full_report_recovers() {
    let transport = ControlledTransport::new(None);
    let (task, mut receiver) = spawn_forwarder(transport.clone());
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;
    transport.push_idle_timeout();
    let offline = next_snapshot(&mut receiver).await;
    assert_eq!(offline.state, "offline");
    assert!(!offline.telemetry_authoritative);

    transport.wait_for_report_waits(2).await;
    transport.push_idle_timeout();
    transport.wait_for_deliveries(2).await;
    transport.wait_for_report_waits(3).await;
    assert!(receiver.try_recv().is_err());

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 1,
            "gcode_state": "RUNNING",
            "ctt": 42
        }
    }));
    let partial = next_snapshot(&mut receiver).await;
    assert!(partial.state.is_empty());
    assert_eq!(partial.chamber_target_temperature_celsius, "42");
    assert!(!partial.telemetry_authoritative);

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": "printer-owned",
            "gcode_state": "IDLE"
        }
    }));
    let recovered = next_snapshot(&mut receiver).await;
    assert_eq!(recovered.state, "IDLE");
    assert!(recovered.telemetry_authoritative);

    transport.push_idle_timeout();
    let offline_again = next_snapshot(&mut receiver).await;
    assert_eq!(offline_again.state, "offline");

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn h2c_periodic_full_reports_remain_authoritative_without_request_correlation() {
    let transport = ControlledTransport::new(None);
    let mut h2c = endpoint();
    h2c.model = Some("O1C2".to_owned());
    let (task, mut receiver) = spawn_forwarder_for_endpoint(transport.clone(), h2c);
    transport.wait_for_publish_attempts(1).await;
    transport.wait_for_report_waits(1).await;

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": "2021",
            "gcode_state": "IDLE"
        }
    }));
    let initial = next_snapshot(&mut receiver).await;
    assert_eq!(initial.state, "IDLE");
    assert!(initial.telemetry_authoritative);
    transport.wait_for_report_waits(2).await;

    transport.push_idle_timeout();
    assert_eq!(next_snapshot(&mut receiver).await.state, "offline");
    transport.wait_for_report_waits(3).await;

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": "2021",
            "gcode_state": "IDLE"
        }
    }));
    let recovered = next_snapshot(&mut receiver).await;
    assert_eq!(recovered.state, "IDLE");
    assert!(recovered.telemetry_authoritative);
    transport.wait_for_report_waits(4).await;

    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": "2021",
            "gcode_state": "RUNNING"
        }
    }));
    let subsequent = next_snapshot(&mut receiver).await;
    assert_eq!(subsequent.state, "RUNNING");
    assert!(subsequent.telemetry_authoritative);

    abort_and_join(task).await;
}

#[tokio::test(start_paused = true)]
async fn transport_failures_emit_offline_once_until_a_full_report_recovers() {
    let transport = ControlledTransport::new(None);
    let config = test_config();
    let endpoint = endpoint();
    let cache = DeviceFeatureCache::default();
    let (sender, mut receiver) = mpsc::channel(128);
    let mut presence = MqttPresenceState::default();

    for attempt in 1..=2 {
        transport.push_transport_failure();
        let error = forward_print_reports_with_context(
            &config,
            &transport,
            &endpoint,
            Duration::from_secs(10),
            &sender,
            MqttForwardingContext {
                device_features: &cache,
                firmware: None,
                presence: &mut presence,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("controlled MQTT transport failure"));
        if attempt == 1 {
            assert_eq!(next_snapshot(&mut receiver).await.state, "offline");
        } else {
            assert!(receiver.try_recv().is_err());
        }
    }

    let task_transport = transport.clone();
    let task_sender = sender.clone();
    let task = tokio::spawn(async move {
        forward_print_reports_with_context(
            &config,
            &task_transport,
            &endpoint,
            Duration::from_secs(10),
            &task_sender,
            MqttForwardingContext {
                device_features: &cache,
                firmware: None,
                presence: &mut presence,
            },
        )
        .await
    });
    transport.wait_for_publish_attempts(3).await;
    transport.wait_for_report_waits(3).await;
    let sequence_id = pushall_sequence_id(&transport.published_commands()[2].1).to_owned();
    transport.push_report(json!({
        "print": {
            "command": "push_status",
            "msg": 0,
            "sequence_id": sequence_id,
            "gcode_state": "IDLE"
        }
    }));
    transport.push_transport_failure();
    let error = task.await.unwrap().unwrap_err();
    assert!(format!("{error:#}").contains("controlled MQTT transport failure"));
    let recovered = next_snapshot(&mut receiver).await;
    assert_eq!(recovered.state, "IDLE");
    assert!(recovered.telemetry_authoritative);
    assert_eq!(next_snapshot(&mut receiver).await.state, "offline");
}
