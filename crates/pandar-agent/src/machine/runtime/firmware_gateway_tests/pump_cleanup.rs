use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use pandar_core::FirmwareCommand;
use tokio::sync::mpsc;

use super::super::firmware_gateway::firmware_publish_transition_with_cleanup;
use super::*;
use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, FirmwareExecuteRequest, FirmwareObservationCache,
        FirmwarePrepareRequest,
    },
};

#[tokio::test]
async fn firmware_cancelled_suback_connect_reaps_pump_and_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (subscribed, wait_subscribed) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 8);
        subscribed.send(()).unwrap();
        assert_socket_closed(&mut stream).await;
    });
    let finished = Arc::new(AtomicBool::new(false));
    let task_set = FirmwareMqttTaskSet::default();
    let connect = tokio::spawn(
        FirmwareMqttSession::connect_with_options_and_pump_finished_and_task_set(
            test_options(address),
            REQUEST_TOPIC.into(),
            REPORT_TOPIC.into(),
            Arc::clone(&finished),
            task_set.clone(),
        ),
    );
    wait_subscribed.await.unwrap();

    connect.abort();
    let _ = connect.await;
    task_set.abort_and_join_all().await.unwrap();

    wait_finished(&finished).await;
    timeout(Duration::from_millis(250), broker)
        .await
        .expect("cancelled SUBACK connect must close its socket")
        .unwrap();
}

#[tokio::test]
async fn firmware_cancelled_connected_operation_reaps_pump_and_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        assert_socket_closed(&mut stream).await;
    });
    let finished = Arc::new(AtomicBool::new(false));
    let task_set = FirmwareMqttTaskSet::default();
    let session = FirmwareMqttSession::connect_with_options_and_pump_finished_and_task_set(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        Arc::clone(&finished),
        task_set.clone(),
    )
    .await
    .unwrap();
    let operation = tokio::spawn(async move {
        let _session = session;
        std::future::pending::<()>().await;
    });

    operation.abort();
    let _ = operation.await;
    task_set.abort_and_join_all().await.unwrap();

    wait_finished(&finished).await;
    timeout(Duration::from_millis(250), broker)
        .await
        .expect("cancelled connected operation must close its socket")
        .unwrap();
}

#[tokio::test]
async fn firmware_cancelled_explicit_shutdown_reaps_pump() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(expect_cleanup_connection(listener));
    let task_set = FirmwareMqttTaskSet::default();
    let mut session = FirmwareMqttSession::connect_with_options_and_task_set(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        task_set.clone(),
    )
    .await
    .unwrap();
    let reaped = session.pump_reaped_flag_for_test();
    let mut join_pause = session.pause_pump_join_for_test().await;
    let shutdown = tokio::spawn(async move { session.shutdown().await });
    join_pause.wait_until_reached().await;

    shutdown.abort();
    let _ = shutdown.await;
    task_set.abort_and_join_all().await.unwrap();

    wait_finished(&reaped).await;
    timeout(Duration::from_millis(250), broker)
        .await
        .expect("cancelled explicit shutdown must reap its pump")
        .unwrap();
}

#[tokio::test]
async fn firmware_shutdown_completion_error_still_joins_pump_before_return() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        assert_socket_closed(&mut stream).await;
    });
    let mut session = connect_session(address).await;
    session.fail_shutdown_completion_for_test();
    let mut join_pause = session.pause_pump_join_for_test().await;
    let shutdown = tokio::spawn(async move { session.shutdown().await });

    join_pause.wait_until_reached().await;
    assert!(
        !shutdown.is_finished(),
        "shutdown must not return while its pump join is paused"
    );
    join_pause.release();
    let error = shutdown.await.unwrap().unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("firmware shutdown completion error sentinel"));
    assert!(message.contains("firmware shutdown pump error sentinel"));
    broker.await.unwrap();
}

#[tokio::test]
async fn firmware_dropped_shutdown_completion_still_joins_pump_before_return() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        assert_socket_closed(&mut stream).await;
    });
    let mut session = connect_session(address).await;
    session.drop_shutdown_completion_for_test();
    let mut join_pause = session.pause_pump_join_for_test().await;
    let shutdown = tokio::spawn(async move { session.shutdown().await });

    join_pause.wait_until_reached().await;
    assert!(
        !shutdown.is_finished(),
        "shutdown must not return while its pump join is paused"
    );
    join_pause.release();
    let error = shutdown.await.unwrap().unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("dropped shutdown completion"));
    assert!(message.contains("firmware shutdown sender-drop pump sentinel"));
    broker.await.unwrap();
}

#[tokio::test]
async fn firmware_invalidated_publish_transition_closes_session_without_publish() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(expect_cleanup_connection(listener));
    let finished = Arc::new(AtomicBool::new(false));
    let mut session = FirmwareMqttSession::connect_with_options_and_pump_finished(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        Arc::clone(&finished),
    )
    .await
    .unwrap();
    let endpoint = endpoint();
    let (cache, execution) = claimed_execution(endpoint.clone()).await;
    cache.cancel_firmware_session(501).await;

    let error =
        match firmware_publish_transition_with_cleanup(&mut session, &execution, &endpoint).await {
            Ok(_) => panic!("invalidated transition must fail"),
            Err(error) => error,
        };

    assert!(format!("{error:#}").contains("no longer current"));
    assert!(session.pump_finished_for_test());
    assert!(finished.load(Ordering::SeqCst));
    broker.await.unwrap();
}

#[tokio::test]
async fn firmware_endpoint_mismatch_releases_transition_before_cleanup_and_never_publishes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(expect_cleanup_connection(listener));
    let finished = Arc::new(AtomicBool::new(false));
    let mut session = FirmwareMqttSession::connect_with_options_and_pump_finished(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        Arc::clone(&finished),
    )
    .await
    .unwrap();
    let current_endpoint = endpoint();
    let (cache, execution) = claimed_execution(current_endpoint.clone()).await;
    let mut stale_endpoint = current_endpoint.clone();
    stale_endpoint.host = "192.0.2.99".into();
    let mut shutdown_pause = session.pause_shutdown_for_test();
    let (events, _event_receiver) = mpsc::channel(2);
    let replacement_endpoint = current_endpoint.clone();

    let (result, ()) = tokio::join!(
        firmware_publish_transition_with_cleanup(&mut session, &execution, &stale_endpoint),
        async {
            shutdown_pause.wait_until_reached().await;
            timeout(
                Duration::from_millis(250),
                cache.begin_generation(&test_config(), replacement_endpoint, &events, Some(1)),
            )
            .await
            .expect("endpoint mismatch must release transition before MQTT cleanup")
            .unwrap()
            .unwrap();
            shutdown_pause.release();
        }
    );
    let error = match result {
        Ok(_) => panic!("endpoint mismatch must fail"),
        Err(error) => error,
    };

    assert!(format!("{error:#}").contains("endpoint changed before publish"));
    assert!(session.pump_finished_for_test());
    assert!(finished.load(Ordering::SeqCst));
    broker.await.unwrap();
}

async fn expect_cleanup_connection(listener: TcpListener) {
    let (mut stream, _) = listener.accept().await.unwrap();
    accept_subscription(&mut stream).await;
    let packet = read_packet(&mut stream).await;
    assert_eq!(packet.header >> 4, 14, "cleanup must not publish firmware");
    assert_socket_closed(&mut stream).await;
}

async fn assert_socket_closed(stream: &mut TcpStream) {
    let mut byte = [0_u8; 1];
    let count = timeout(Duration::from_millis(250), stream.read(&mut byte))
        .await
        .expect("firmware pump must close the broker socket")
        .unwrap();
    assert_eq!(count, 0);
}

async fn wait_finished(finished: &AtomicBool) {
    timeout(Duration::from_millis(250), async {
        while !finished.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("firmware pump must be joined or reaped");
}

async fn claimed_execution(
    endpoint: BambuPrinterEndpoint,
) -> (
    FirmwareObservationCache,
    crate::machine::FirmwareExecutionLease,
) {
    let cache = FirmwareObservationCache::default();
    let (events, _event_receiver) = mpsc::channel(2);
    let transition = cache
        .begin_generation(&test_config(), endpoint.clone(), &events, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "cleanup".into(),
            serial: endpoint.serial.clone(),
            expected_generation: generation,
            session_epoch: 501,
        })
        .await
        .unwrap();
    let execution = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "cleanup".into(),
            serial: endpoint.serial,
            expected_generation: generation,
            session_epoch: 501,
            command: FirmwareCommand::UpgradeConfirm {
                sequence_id: "cleanup".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap();
    (cache, execution)
}

fn test_options(address: std::net::SocketAddr) -> MqttOptions {
    let mut options = MqttOptions::new(
        format!("firmware-cleanup-test-{}", uuid::Uuid::new_v4()),
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(30)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    options
}

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".into(),
        serial: "SERIAL".into(),
        access_code: "secret".into(),
        model: Some("X1".into()),
        name: Some("office".into()),
    }
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.invalid".into(),
        hub_api_url: None,
        agent_name: "test".into(),
        agent_id: "agent".into(),
        tenant_id: "tenant".into(),
        agent_credential: "credential".into(),
        agent_version: "test".into(),
        printers: "[]".into(),
    }
}
