use std::{sync::Arc, time::Duration};

use tokio::sync::{Notify, mpsc};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};

use super::*;

mod support;
use support::*;
mod heartbeat_join;
mod pump_ownership;
mod report_ownership;

#[tokio::test]
async fn firmware_cancelling_run_once_during_prepare_runs_exact_teardown() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(CancellationAgentControlService {
                    connected,
                    inbound_closed,
                }))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        }
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        Vec::new(),
        Duration::from_secs(1),
    ));
    let mut prepare = gateway.pause_prepare_session_for_test().await;
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    prepare.wait_until_reached().await;
    assert!(gateway.has_current_sender_for_test().await);
    tokio::time::timeout(Duration::from_secs(1), async {
        while active_heartbeat_tasks_for_test() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    task.abort();
    let _ = task.await;

    tokio::time::timeout(Duration::from_millis(250), async {
        inbound_closed.notified().await;
        prepare.wait_until_dropped().await;
        while active_heartbeat_tasks_for_test() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled run_once must not leave heartbeat or stream senders detached");
    assert!(!gateway.has_current_sender_for_test().await);
    assert_ne!(gateway.firmware_cache().ended_session_epoch_for_test(), 0);
    server.abort();
    let _ = server.await;
}

#[test]
fn firmware_session_supervisor_reaper_logs_inner_error() {
    let (logs, ()) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let task = tokio::spawn(async {
                    Err::<RunOutcome, _>(anyhow::anyhow!(
                        "firmware supervisor inner error sentinel"
                    ))
                });
                reap_session_task(task).await;
            });
    });

    assert!(
        logs.contents()
            .contains("firmware supervisor inner error sentinel")
    );
}

#[tokio::test]
async fn firmware_partial_multi_printer_prepare_joins_registered_report_forwarder() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(CancellationAgentControlService {
                    connected,
                    inbound_closed,
                }))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        }
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![runtime_endpoint("FIRST"), runtime_endpoint("SECOND")],
        Duration::from_secs(1),
    ));
    let mut partial = gateway
        .fail_prepare_after_first_report_forwarder_for_test()
        .await;
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    partial.wait_until_registered().await;

    partial.fail();
    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("partial prepare failure must run report teardown")
        .unwrap()
        .unwrap_err();

    assert!(format!("{error:#}").contains("partial prepare failure"));
    assert!(partial.forwarder_was_dropped());
    tokio::time::timeout(Duration::from_millis(250), inbound_closed.notified())
        .await
        .expect("partial prepare teardown must close old Hub inbound");
    assert!(!gateway.has_current_sender_for_test().await);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn firmware_partial_prepare_cancellation_joins_registered_report_forwarder() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(CancellationAgentControlService {
                    connected,
                    inbound_closed,
                }))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        }
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![runtime_endpoint("FIRST"), runtime_endpoint("SECOND")],
        Duration::from_secs(1),
    ));
    let mut partial = gateway
        .fail_prepare_after_first_report_forwarder_for_test()
        .await;
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    partial.wait_until_registered().await;

    task.abort();
    let _ = task.await;

    tokio::time::timeout(Duration::from_millis(250), async {
        while !partial.forwarder_was_dropped() {
            tokio::task::yield_now().await;
        }
        inbound_closed.notified().await;
    })
    .await
    .expect("partial prepare cancellation must join report forwarder and close Hub inbound");
    assert!(!gateway.has_current_sender_for_test().await);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn firmware_command_eof_joins_report_forwarder_before_return() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let end_commands = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        let end_commands = Arc::clone(&end_commands);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(EofAgentControlService {
                    connected,
                    inbound_closed,
                    end_commands,
                }))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        }
    });
    let config = AgentConfig {
        hub_grpc_url: format!("http://127.0.0.1:{}", address.port()),
        ..test_config()
    };
    let gateway = Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        Vec::new(),
        Duration::from_secs(1),
    ));
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    tokio::time::timeout(Duration::from_millis(250), async {
        while !gateway.has_current_sender_for_test().await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let report = gateway
        .install_blocking_report_forwarder_for_test("EOF")
        .await;

    end_commands.notify_one();
    let outcome = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("command EOF must join report forwarders")
        .unwrap()
        .unwrap();

    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
    assert!(report.was_dropped());
    tokio::time::timeout(Duration::from_millis(250), inbound_closed.notified())
        .await
        .expect("EOF teardown must close old Hub inbound");
    assert!(!gateway.has_current_sender_for_test().await);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn blocked_normal_command_does_not_delay_firmware_prepare_or_eof_teardown() {
    let transport = BlockingMqttTransport::default();
    let gateway = gateway(transport.clone());
    let generation = seed_firmware_generation(&gateway).await;
    let (events, mut event_receiver) = mpsc::channel(16);
    let (commands, command_receiver) = mpsc::channel(4);
    let task = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                71,
            )
            .await
        }
    });
    commands.send(Ok(refresh_command())).await.unwrap();
    transport.wait_until_blocked().await;
    commands
        .send(Ok(prepare_command("prepare-1", "prepare-1", generation)))
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let event = event_receiver.recv().await.unwrap();
            if matches!(event.event, Some(agent_event::Event::FirmwarePrepared(_))) {
                break;
            }
        }
    })
    .await
    .expect("firmware prepare must not wait for the ordered normal worker");

    drop(commands);
    let outcome = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("EOF must abort and join a blocked normal worker")
        .unwrap()
        .unwrap();
    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
    assert!(transport.was_cancelled());
}

#[tokio::test]
async fn stream_read_error_aborts_and_joins_blocked_normal_worker() {
    let transport = BlockingMqttTransport::default();
    let gateway = gateway(transport.clone());
    let (events, _event_receiver) = mpsc::channel(16);
    let (commands, command_receiver) = mpsc::channel(4);
    let task = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                72,
            )
            .await
        }
    });
    commands.send(Ok(refresh_command())).await.unwrap();
    transport.wait_until_blocked().await;
    commands
        .send(Err(Status::unavailable("reverse stream failed")))
        .await
        .unwrap();

    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("stream error cleanup must join the normal worker")
        .unwrap()
        .unwrap_err();

    assert!(format!("{error:#}").contains("reverse stream failed"));
    assert!(transport.was_cancelled());
}

#[tokio::test]
async fn firmware_protocol_rejection_is_acked_and_keeps_stream_alive() {
    let transport = BlockingMqttTransport::default();
    let gateway = gateway(transport);
    let generation = seed_firmware_generation(&gateway).await;
    let (events, mut event_receiver) = mpsc::channel(16);
    let (commands, command_receiver) = mpsc::channel(2);
    let task = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                73,
            )
            .await
        }
    });
    commands
        .send(Ok(prepare_command("outer-id", "inner-id", generation)))
        .await
        .unwrap();

    match event_receiver.recv().await.unwrap().event {
        Some(agent_event::Event::CommandAck(ack)) => {
            assert!(!ack.accepted);
            assert!(ack.error.contains("outer command id"));
        }
        other => panic!("expected rejected ack, got {other:?}"),
    }

    drop(commands);
    let outcome = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("protocol rejection must not stop stream reading")
        .unwrap()
        .unwrap();
    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
}

#[tokio::test]
async fn firmware_runtime_error_stops_stream_and_is_not_discarded() {
    let gateway = gateway(BlockingMqttTransport::default());
    let (events, event_receiver) = mpsc::channel(1);
    drop(event_receiver);
    let (commands, command_receiver) = mpsc::channel(2);
    let task = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                73,
            )
            .await
        }
    });
    commands
        .send(Ok(execute_command("runtime-id")))
        .await
        .unwrap();

    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("firmware runtime errors must stop stream reading")
        .unwrap()
        .unwrap_err();

    assert!(format!("{error:#}").contains("run firmware command task"));
    drop(commands);
}

#[tokio::test]
async fn normal_handler_error_stops_stream_and_is_not_discarded() {
    let gateway = gateway(BlockingMqttTransport::default());
    let (events, event_receiver) = mpsc::channel(1);
    drop(event_receiver);
    let (commands, command_receiver) = mpsc::channel(2);
    let task = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                74,
            )
            .await
        }
    });
    commands.send(Ok(refresh_command())).await.unwrap();

    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("normal handler errors must stop stream reading")
        .unwrap()
        .unwrap_err();

    assert!(format!("{error:#}").contains("queue refresh-printers command ack"));
    drop(commands);
}

#[tokio::test]
async fn old_epoch_firmware_task_cannot_publish_or_emit_into_replacement_stream() {
    let gateway = gateway(BlockingMqttTransport::default());
    let generation = seed_firmware_generation(&gateway).await;
    gateway
        .firmware_cache()
        .prepare_firmware_control(crate::machine::FirmwarePrepareRequest {
            command_id: "old-execute".into(),
            serial: "SERIAL".into(),
            expected_generation: generation,
            session_epoch: 75,
        })
        .await
        .unwrap();
    let pause = gateway.pause_firmware_execute().await;
    let (old_events, _old_receiver) = mpsc::channel(8);
    let (old_commands, old_command_receiver) = mpsc::channel(2);
    let old = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &old_events,
                ReceiverStream::new(old_command_receiver),
                75,
            )
            .await
        }
    });
    old_commands
        .send(Ok(execute_command("old-execute")))
        .await
        .unwrap();
    pause.wait_until_blocked().await;
    drop(old_commands);

    tokio::time::timeout(Duration::from_millis(250), old)
        .await
        .expect("old firmware task must be aborted and joined before replacement")
        .unwrap()
        .unwrap();
    assert!(pause.was_cancelled());
    assert!(
        gateway
            .firmware_cache()
            .snapshot("SERIAL")
            .await
            .unwrap()
            .reservation
            .is_none()
    );
    pause.release();

    let (replacement_sender, mut replacement_events) = mpsc::channel(8);
    handle_command_stream_with_gateway(
        &test_config(),
        Arc::clone(&gateway),
        &replacement_sender,
        tokio_stream::empty::<Result<HubCommand, Status>>(),
        76,
    )
    .await
    .unwrap();

    assert_eq!(gateway.firmware_publish_count(), 0);
    assert!(replacement_events.try_recv().is_err());
}
