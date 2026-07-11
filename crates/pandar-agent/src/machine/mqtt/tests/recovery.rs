use std::time::Duration;

use rumqttc::{Event, Outgoing, Packet, PubAck, Publish, QoS};
use tokio::{net::TcpListener, sync::oneshot};

use super::*;
use crate::machine::{
    PrinterOperation,
    mqtt::recovery::{
        RecoveryAttempt, dispatch_rumqttc_attempt, dispatch_with_attempt, dispatch_with_deadline,
        recovery_mqtt_options,
    },
    operations::mqtt_command_for_printer_operation,
};

mod broker;
mod fake;

use broker::*;
use fake::*;

#[tokio::test]
async fn recovery_waits_for_its_own_publish_puback_and_ignores_reports() {
    let (attempt, probe) = FakeRecoveryAttempt::new([
        PollStep::Event(application_report("resume")),
        PollStep::Event(Event::Outgoing(Outgoing::Publish(41))),
        PollStep::Event(application_report("ignore")),
        PollStep::Event(Event::Incoming(Packet::PubAck(PubAck::new(41)))),
        PollStep::Event(application_report("stop")),
    ]);

    let result = dispatch_with_attempt(
        attempt,
        "device/01S00EXAMPLE/request".to_owned(),
        recovery_command(PrintErrorAction::Resume),
    )
    .await
    .unwrap();

    assert_eq!(result.sequence_id.as_deref(), Some("0"));
    assert_eq!(probe.poll_calls(), 4);
    assert_eq!(probe.unpolled_events_on_drop(), 1);
    assert!(probe.was_dropped());
}

#[tokio::test]
async fn timed_out_unacknowledged_attempt_is_dropped_before_retry() {
    let endpoint = endpoint();
    let first_options = recovery_mqtt_options(&endpoint);
    let second_options = recovery_mqtt_options(&endpoint);
    let (first, first_probe) = FakeRecoveryAttempt::new([
        PollStep::Event(Event::Outgoing(Outgoing::Publish(3))),
        PollStep::Pending,
    ]);

    let err = dispatch_with_deadline(
        dispatch_with_attempt(
            first,
            "device/01S00EXAMPLE/request".to_owned(),
            recovery_command(PrintErrorAction::Ignore),
        ),
        Duration::from_millis(5),
    )
    .await
    .unwrap_err();

    assert!(
        format!("{err:#}")
            .contains("timed out dispatching sequence-zero recovery through MQTT PUBACK")
    );
    assert!(first_probe.was_dropped());
    assert_eq!(first_probe.poll_calls(), 2);
    assert_eq!(first_probe.unpolled_events_on_drop(), 0);

    let (retry, retry_probe) = FakeRecoveryAttempt::new([
        PollStep::Event(Event::Outgoing(Outgoing::Publish(9))),
        PollStep::Event(Event::Incoming(Packet::PubAck(PubAck::new(9)))),
    ]);

    let result = dispatch_with_attempt(
        retry,
        "device/01S00EXAMPLE/request".to_owned(),
        recovery_command(PrintErrorAction::Ignore),
    )
    .await
    .unwrap();

    assert_eq!(result.sequence_id.as_deref(), Some("0"));
    assert_eq!(retry_probe.poll_calls(), 2);
    assert_ne!(first_options.client_id(), second_options.client_id());
}

#[tokio::test]
async fn no_event_loop_progress_cannot_complete_recovery() {
    let (attempt, probe) = FakeRecoveryAttempt::new([PollStep::Pending]);

    let err = dispatch_with_deadline(
        dispatch_with_attempt(
            attempt,
            "device/01S00EXAMPLE/request".to_owned(),
            recovery_command(PrintErrorAction::Resume),
        ),
        Duration::from_millis(5),
    )
    .await
    .unwrap_err();

    assert!(
        format!("{err:#}")
            .contains("timed out dispatching sequence-zero recovery through MQTT PUBACK")
    );
    assert_eq!(probe.publish_calls(), 1);
    assert_eq!(probe.poll_calls(), 1);
    assert!(probe.was_dropped());
}

#[tokio::test]
async fn recovery_attempt_isolated_from_reusable_connection_unacknowledged_work() {
    let endpoint = endpoint();
    let reusable_options = bambu_lan_mqtt_options(&endpoint, None);
    let first = recovery_mqtt_options(&endpoint);
    let second = recovery_mqtt_options(&endpoint);
    let (mut reusable, reusable_probe) = FakeRecoveryAttempt::new([
        PollStep::Event(Event::Outgoing(Outgoing::Publish(77))),
        PollStep::Pending,
    ]);
    reusable
        .publish(
            "device/01S00EXAMPLE/request".to_owned(),
            QoS::AtLeastOnce,
            false,
            br#"{"print":{"command":"pause","sequence_id":"20001"}}"#.to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(
        reusable.poll().await.unwrap(),
        Event::Outgoing(Outgoing::Publish(77))
    );

    let (recovery, recovery_probe) = FakeRecoveryAttempt::new([
        PollStep::Event(Event::Outgoing(Outgoing::Publish(8))),
        PollStep::Event(Event::Incoming(Packet::PubAck(PubAck::new(8)))),
    ]);
    let result = dispatch_with_attempt(
        recovery,
        "device/01S00EXAMPLE/request".to_owned(),
        recovery_command(PrintErrorAction::Resume),
    )
    .await
    .unwrap();

    assert!(first.clean_session());
    assert!(second.clean_session());
    assert_ne!(first.client_id(), reusable_options.client_id());
    assert_ne!(first.client_id(), second.client_id());
    assert!(first.client_id().contains("-recovery-"));
    assert_eq!(result.sequence_id.as_deref(), Some("0"));
    assert_eq!(recovery_probe.publish_calls(), 1);
    assert_eq!(reusable_probe.publish_calls(), 1);
    assert_eq!(reusable_probe.poll_calls(), 1);
    assert!(!reusable_probe.was_dropped());
    drop(reusable);
    assert!(reusable_probe.was_dropped());
}

#[tokio::test]
async fn production_boundary_enqueues_one_publish_and_no_subscribe() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (dispatch_done, dispatch_observed) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let mut connection = accept_connection(&listener, "recovery-boundary").await;
        let publish = read_publish(&mut connection).await;
        assert_no_packet_before_ack(&mut connection).await;
        send_puback(&mut connection, publish.packet_id)
            .await
            .unwrap();
        dispatch_observed.await.unwrap();
        publish
    });

    let result = dispatch_rumqttc_attempt(
        local_mqtt_options(address, "recovery-boundary"),
        TEST_REQUEST_TOPIC.to_owned(),
        recovery_command(PrintErrorAction::Resume),
    )
    .await
    .unwrap();
    dispatch_done.send(()).unwrap();
    let publish = broker.await.unwrap();

    assert_eq!(result.sequence_id.as_deref(), Some("0"));
    assert_eq!(publish.topic, TEST_REQUEST_TOPIC);
    assert_eq!(publish.qos, 1);
    assert!(!publish.retain);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&publish.payload).unwrap(),
        serde_json::json!({
            "print": {
                "command": "resume",
                "err": "83918929",
                "job_id": "job-7",
                "param": "reserve",
                "sequence_id": "0"
            }
        })
    );
}

#[tokio::test]
async fn unsolicited_puback_drops_old_connection_and_cannot_cross_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (old_ack_sent, old_ack_observed) = oneshot::channel();
    let (release_fresh_ack, fresh_ack_released) = oneshot::channel();
    let (retry_done, retry_observed) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let mut old_connection = accept_connection(&listener, "recovery-old").await;
        let old_publish = read_publish(&mut old_connection).await;
        let unsolicited_packet_id = if old_publish.packet_id == u16::MAX {
            1
        } else {
            old_publish.packet_id + 1
        };
        send_puback(&mut old_connection, unsolicited_packet_id)
            .await
            .unwrap();

        let mut fresh_connection = accept_connection(&listener, "recovery-fresh").await;
        let fresh_publish = read_publish(&mut fresh_connection).await;
        let _ = send_puback(&mut old_connection, old_publish.packet_id).await;
        old_ack_sent.send(()).unwrap();
        fresh_ack_released.await.unwrap();
        send_puback(&mut fresh_connection, fresh_publish.packet_id)
            .await
            .unwrap();
        retry_observed.await.unwrap();
    });

    let err = dispatch_rumqttc_attempt(
        local_mqtt_options(address, "recovery-old"),
        TEST_REQUEST_TOPIC.to_owned(),
        recovery_command(PrintErrorAction::Ignore),
    )
    .await
    .unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("poll recovery MQTT event loop"));
    assert!(chain.contains("Mqtt state: Received unsolicited ack pkid:"));

    let mut retry = tokio::spawn(dispatch_rumqttc_attempt(
        local_mqtt_options(address, "recovery-fresh"),
        TEST_REQUEST_TOPIC.to_owned(),
        recovery_command(PrintErrorAction::Ignore),
    ));
    old_ack_observed.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut retry)
            .await
            .is_err()
    );
    release_fresh_ack.send(()).unwrap();

    let result = retry.await.unwrap().unwrap();
    assert_eq!(result.sequence_id.as_deref(), Some("0"));
    retry_done.send(()).unwrap();
    broker.await.unwrap();
}

#[tokio::test]
async fn recovery_actions_enqueue_one_studio_payload_at_qos_one_without_retain() {
    for (action, expected_command) in [
        (PrintErrorAction::Resume, "resume"),
        (PrintErrorAction::Ignore, "ignore"),
        (PrintErrorAction::Stop, "stop"),
    ] {
        let (attempt, probe) = FakeRecoveryAttempt::new([
            PollStep::Event(Event::Outgoing(Outgoing::Publish(12))),
            PollStep::Event(Event::Incoming(Packet::PubAck(PubAck::new(12)))),
        ]);

        let result = dispatch_with_attempt(
            attempt,
            "device/mqtt-serial/request".to_owned(),
            recovery_command(action),
        )
        .await
        .unwrap();

        assert_eq!(result.sequence_id.as_deref(), Some("0"));
        let publishes = probe.publishes();
        assert_eq!(publishes.len(), 1);
        assert_eq!(publishes[0].topic, "device/mqtt-serial/request");
        assert_eq!(publishes[0].qos, QoS::AtLeastOnce);
        assert!(!publishes[0].retain);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&publishes[0].payload).unwrap(),
            serde_json::json!({
                "print": {
                    "command": expected_command,
                    "err": "83918929",
                    "job_id": "job-7",
                    "param": "reserve",
                    "sequence_id": "0"
                }
            })
        );
    }
}

#[tokio::test]
async fn queue_error_drops_attempt_without_polling_and_keeps_context() {
    let (mut attempt, probe) = FakeRecoveryAttempt::new([]);
    attempt.publish_error = Some("request channel closed");

    let err = dispatch_with_attempt(
        attempt,
        "device/01S00EXAMPLE/request".to_owned(),
        recovery_command(PrintErrorAction::Resume),
    )
    .await
    .unwrap_err();

    let chain = format!("{err:#}");
    assert!(chain.contains("enqueue sequence-zero recovery MQTT publish"));
    assert!(chain.contains("request channel closed"));
    assert_eq!(probe.publish_calls(), 1);
    assert_eq!(probe.poll_calls(), 0);
    assert!(probe.was_dropped());
}

#[tokio::test]
async fn connect_poll_and_protocol_errors_drop_attempt_and_keep_context() {
    for cause in [
        "connection refused",
        "socket read failed",
        "unsolicited PUBACK protocol error",
    ] {
        let (attempt, probe) = FakeRecoveryAttempt::new([PollStep::Error(cause)]);

        let err = dispatch_with_attempt(
            attempt,
            "device/01S00EXAMPLE/request".to_owned(),
            recovery_command(PrintErrorAction::Resume),
        )
        .await
        .unwrap_err();

        let chain = format!("{err:#}");
        assert!(chain.contains("poll recovery MQTT event loop"));
        assert!(chain.contains(cause));
        assert!(probe.was_dropped());
    }
}

#[tokio::test]
async fn cancellation_drops_the_whole_recovery_connection() {
    let (attempt, probe) = FakeRecoveryAttempt::new([PollStep::Pending]);
    let poll_started = probe.poll_started();
    let task = tokio::spawn(dispatch_with_attempt(
        attempt,
        "device/01S00EXAMPLE/request".to_owned(),
        recovery_command(PrintErrorAction::Resume),
    ));

    poll_started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(probe.was_dropped());
}

fn recovery_command(action: PrintErrorAction) -> BambuMqttCommand {
    mqtt_command_for_printer_operation(PrinterOperation::HandlePrintError {
        error_action: action,
        print_error: 83_918_929,
        printer_job_id: "job-7".to_owned(),
        sequence_id: 0,
    })
    .unwrap()
}

fn application_report(command: &str) -> Event {
    Event::Incoming(Packet::Publish(Publish::new(
        "device/01S00EXAMPLE/report",
        QoS::AtMostOnce,
        serde_json::to_vec(&serde_json::json!({
            "print": {"command": command, "sequence_id": "0"}
        }))
        .unwrap(),
    )))
}
