use super::*;

fn runtime_gateway() -> Arc<crate::machine::runtime::RuntimeBambuMachineGateway> {
    Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        test_config(),
        Vec::new(),
        Duration::from_secs(1),
    ))
}

async fn prepare_empty_session(
    gateway: &crate::machine::runtime::RuntimeBambuMachineGateway,
) -> mpsc::Sender<AgentEvent> {
    let (sender, _events) = mpsc::channel(4);
    gateway.prepare_session(&sender).await.unwrap();
    sender
}

async fn wait_for_session_sender(gateway: &crate::machine::runtime::RuntimeBambuMachineGateway) {
    tokio::time::timeout(Duration::from_millis(250), async {
        while !gateway.has_current_sender_for_test().await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime session sender must become active");
}

async fn assert_session_cleanup_waits_for_report_join(
    gateway: &crate::machine::runtime::RuntimeBambuMachineGateway,
) {
    assert!(gateway.has_current_sender_for_test().await);
    assert_eq!(
        gateway.firmware_cache().ended_session_epoch_for_test(),
        0,
        "firmware epoch cancellation must wait for report join"
    );
}

#[tokio::test]
async fn firmware_report_teardown_cancellation_retains_handle_for_retry() {
    let gateway = runtime_gateway();
    let _sender = prepare_empty_session(&gateway).await;
    let report = gateway
        .install_blocking_report_forwarder_for_test("TEARDOWN")
        .await;
    let pause = gateway.pause_report_join_for_test("TEARDOWN").await;
    let teardown = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move { gateway.teardown_session_report_forwarders().await }
    });
    pause.wait_until_reached().await;

    teardown.abort();
    let _ = teardown.await;

    assert!(
        gateway.has_report_forwarder_for_test("TEARDOWN").await,
        "cancelled teardown must leave the JoinHandle under shared ownership"
    );
    assert!(!report.was_dropped());
    gateway.teardown_session_report_forwarders().await.unwrap();
    assert!(report.was_dropped());
    assert!(!gateway.has_report_forwarder_for_test("TEARDOWN").await);
}

#[tokio::test]
async fn firmware_same_serial_report_replacement_cancellation_keeps_old_handle_reapable() {
    let gateway = runtime_gateway();
    let _sender = prepare_empty_session(&gateway).await;
    let old = gateway
        .install_blocking_report_forwarder_for_test("REPLACE")
        .await;
    let pause = gateway.pause_report_join_for_test("REPLACE").await;
    let replacement = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move { gateway.replace_report_forwarder_for_test("REPLACE").await }
    });
    pause.wait_until_reached().await;

    replacement.abort();
    let _ = replacement.await;
    gateway.teardown_session_report_forwarders().await.unwrap();

    assert!(
        old.was_dropped(),
        "unified teardown must still find and join the replaced forwarder"
    );
}

#[tokio::test]
async fn firmware_report_teardown_preserves_panicking_join_error_cause() {
    let gateway = runtime_gateway();
    gateway
        .install_panicking_report_forwarder_for_test("PANIC")
        .await;

    let error = gateway
        .teardown_session_report_forwarders()
        .await
        .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("join runtime printer report forwarder"));
    assert!(message.contains("firmware report forwarder panic sentinel"));
}

#[tokio::test]
async fn firmware_command_eof_joins_report_before_epoch_cancel_and_sender_clear() {
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
    wait_for_session_sender(&gateway).await;
    let report = gateway
        .install_blocking_report_forwarder_for_test("EOF-ORDER")
        .await;
    let pause = gateway.pause_report_join_for_test("EOF-ORDER").await;

    end_commands.notify_one();
    pause.wait_until_reached().await;
    assert!(!report.was_dropped());
    assert_session_cleanup_waits_for_report_join(&gateway).await;
    pause.release();

    let outcome = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("EOF cleanup must finish after report join")
        .unwrap()
        .unwrap();
    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
    assert!(report.was_dropped());
    assert!(!gateway.has_current_sender_for_test().await);
    assert_ne!(gateway.firmware_cache().ended_session_epoch_for_test(), 0);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn firmware_command_status_joins_report_before_epoch_cancel_and_sender_clear() {
    let connected = Arc::new(Notify::new());
    let inbound_closed = Arc::new(Notify::new());
    let fail_commands = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let connected = Arc::clone(&connected);
        let inbound_closed = Arc::clone(&inbound_closed);
        let fail_commands = Arc::clone(&fail_commands);
        async move {
            tonic::transport::Server::builder()
                .add_service(AgentControlServer::new(StatusAgentControlService {
                    connected,
                    inbound_closed,
                    fail_commands,
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
    wait_for_session_sender(&gateway).await;
    let report = gateway
        .install_blocking_report_forwarder_for_test("STATUS-ORDER")
        .await;
    let pause = gateway.pause_report_join_for_test("STATUS-ORDER").await;

    fail_commands.notify_one();
    pause.wait_until_reached().await;
    assert!(!report.was_dropped());
    assert_session_cleanup_waits_for_report_join(&gateway).await;
    pause.release();

    let error = tokio::time::timeout(Duration::from_millis(250), task)
        .await
        .expect("Status cleanup must finish after report join")
        .unwrap()
        .unwrap_err();
    assert!(format!("{error:#}").contains("firmware report stream status sentinel"));
    assert!(report.was_dropped());
    assert!(!gateway.has_current_sender_for_test().await);
    assert_ne!(gateway.firmware_cache().ended_session_epoch_for_test(), 0);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn firmware_session_cancellation_joins_report_before_epoch_cancel_and_sender_clear() {
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
    let task = tokio::spawn(run_once(config, Arc::clone(&gateway)));
    connected.notified().await;
    wait_for_session_sender(&gateway).await;
    let report = gateway
        .install_blocking_report_forwarder_for_test("CANCEL-ORDER")
        .await;
    let pause = gateway.pause_report_join_for_test("CANCEL-ORDER").await;

    task.abort();
    let _ = task.await;
    pause.wait_until_reached().await;
    assert!(!report.was_dropped());
    assert_session_cleanup_waits_for_report_join(&gateway).await;
    pause.release();

    tokio::time::timeout(Duration::from_millis(250), async {
        while !report.was_dropped()
            || gateway.has_current_sender_for_test().await
            || gateway.firmware_cache().ended_session_epoch_for_test() == 0
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cooperative cancellation cleanup must finish after report join");
    server.abort();
    let _ = server.await;
}
