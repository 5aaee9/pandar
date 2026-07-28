use std::time::Duration;

use rumqttc::MqttOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;

use super::*;
use crate::{
    command_stream::run_command_stream_until_cancelled,
    machine::{
        FirmwareMachineGateway, FirmwarePrepareRequest,
        mqtt::{
            FirmwareMqttSession, FirmwareMqttTaskSet, firmware_barrier_pause,
            firmware_pump_drop_pause,
        },
    },
};

const REQUEST_TOPIC: &str = "device/SERIAL/request";
const REPORT_TOPIC: &str = "device/SERIAL/report";

#[tokio::test]
async fn firmware_command_stream_teardown_joins_pump_before_epoch_cancel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        let mut byte = [0_u8; 1];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
    });
    let gateway = gateway(BlockingMqttTransport::default());
    let generation = seed_firmware_generation(&gateway).await;
    gateway
        .firmware_cache()
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "pump-owner".into(),
            serial: "SERIAL".into(),
            expected_generation: generation,
            session_epoch: 91,
        })
        .await
        .unwrap();
    let task_set = FirmwareMqttTaskSet::default();
    let mut session = connect_session(address, task_set.clone()).await;
    let mut join_pause = session.pause_pump_join_for_test().await;
    session.panic_pump_for_test().await;
    let execute = gateway
        .install_firmware_session_for_execute(session, task_set)
        .await;
    let (events, _event_receiver) = mpsc::channel(8);
    let (commands, command_receiver) = mpsc::channel(2);
    let stream = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            handle_command_stream_with_gateway(
                &test_config(),
                gateway,
                &events,
                ReceiverStream::new(command_receiver),
                91,
            )
            .await
        }
    });
    commands
        .send(Ok(execute_command("pump-owner")))
        .await
        .unwrap();
    execute.wait_until_started().await;

    drop(commands);
    join_pause.wait_until_reached().await;
    assert!(
        timeout(Duration::from_millis(50), async {
            while !stream.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_err(),
        "command-stream teardown must remain blocked on the pump join"
    );
    assert_eq!(gateway.firmware_cache().ended_session_epoch_for_test(), 0);
    join_pause.release();

    let error = stream.await.unwrap().unwrap_err();
    assert!(
        format!("{error:#}").contains("firmware parent-owned pump panic sentinel"),
        "{error:#}"
    );
    assert_eq!(gateway.firmware_cache().ended_session_epoch_for_test(), 91);
    broker.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn firmware_stream_cancellation_aborts_queued_publish_before_generation_release() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        accept_subscription(&mut stream).await;
        let mut byte = [0_u8; 1];
        let count = timeout(Duration::from_secs(5), stream.read(&mut byte))
            .await
            .expect("cancelled firmware pump must close before report teardown resumes")
            .unwrap();
        assert_eq!(count, 0, "cancelled old generation must not publish");
    });
    let gateway = gateway(BlockingMqttTransport::default());
    let generation = seed_firmware_generation(&gateway).await;
    gateway
        .firmware_cache()
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "cancel-before-publish".into(),
            serial: "SERIAL".into(),
            expected_generation: generation,
            session_epoch: 92,
        })
        .await
        .unwrap();
    let task_set = FirmwareMqttTaskSet::default();
    let (barrier, mut barrier_handle) = firmware_barrier_pause();
    let (pump_drop_pause, pump_drop_pause_handle) = firmware_pump_drop_pause();
    let session = FirmwareMqttSession::connect_with_options_and_barrier_pause_and_task_set(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        barrier,
        pump_drop_pause,
        task_set.clone(),
    )
    .await
    .unwrap();
    let abort_requested = session.pump_abort_requested_flag_for_test();
    let pump_reaped = session.pump_reaped_flag_for_test();
    let abort_before_transition_release = Arc::new(AtomicBool::new(false));
    let execute = gateway
        .install_firmware_publish_session_for_execute(
            session,
            task_set.clone(),
            Arc::clone(&abort_before_transition_release),
        )
        .await;
    gateway.report_tasks.lock().await.insert(
        "BLOCKED-REPORT".into(),
        tokio::spawn(std::future::pending()),
    );
    let report_guard = gateway.report_tasks.lock().await;
    let (report_teardown_started, wait_report_teardown_started) = oneshot::channel();
    let report_teardown = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            let _ = report_teardown_started.send(());
            let tasks = gateway
                .report_tasks
                .lock()
                .await
                .drain()
                .map(|(_, task)| task)
                .collect::<Vec<_>>();
            for task in tasks {
                task.abort();
                let _ = task.await;
            }
        }
    });
    wait_report_teardown_started.await.unwrap();

    let (events, _event_receiver) = mpsc::channel(8);
    let (commands, command_receiver) = mpsc::channel(2);
    let (cancel, cancelled) = oneshot::channel();
    let stream_events = events.clone();
    let stream = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            run_command_stream_until_cancelled(
                &test_config(),
                gateway,
                &stream_events,
                ReceiverStream::new(command_receiver),
                92,
                async move {
                    let _ = cancelled.await;
                },
            )
            .await
        }
    });
    commands
        .send(Ok(execute_command("cancel-before-publish")))
        .await
        .unwrap();
    execute.wait_until_started().await;
    barrier_handle.wait_until_reached().await;

    let mut replacement_endpoint = runtime_endpoint("SERIAL");
    replacement_endpoint.host = "192.0.2.99".into();
    let replacement = tokio::spawn({
        let cache = gateway.firmware_cache();
        let events = events.clone();
        async move {
            cache
                .begin_generation(
                    &test_config(),
                    replacement_endpoint,
                    &events,
                    Some(generation),
                )
                .await
        }
    });
    tokio::task::yield_now().await;
    assert!(
        !replacement.is_finished(),
        "generation replacement must wait for the publish transition"
    );

    cancel.send(()).unwrap();
    timeout(
        Duration::from_secs(5),
        pump_drop_pause_handle.wait_until_reached(),
    )
    .await
    .expect("pump cancellation must reach the paused future drop");
    assert!(abort_requested.load(Ordering::SeqCst));
    assert!(
        !replacement.is_finished(),
        "generation replacement must remain blocked until the pump future actually drops"
    );
    assert!(
        !pump_reaped.load(Ordering::SeqCst),
        "the registered pump must remain owned until its paused future drops"
    );
    pump_drop_pause_handle.release();
    let outcome = timeout(Duration::from_secs(5), stream)
        .await
        .expect("firmware command cancellation must not wait for report teardown")
        .unwrap()
        .unwrap();
    assert_eq!(outcome, RunOutcome::ConnectedThenEnded);
    timeout(Duration::from_secs(5), replacement)
        .await
        .expect("cancelled firmware command must release the generation transition")
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        abort_before_transition_release.load(Ordering::SeqCst),
        "pump abort must be requested before the generation transition is released"
    );

    broker
        .await
        .expect("pump socket must close while report teardown remains blocked");
    drop(report_guard);
    report_teardown.await.unwrap();
    barrier_handle.cancel();
    FirmwareMachineGateway::cancel_firmware_session(gateway.as_ref(), 92)
        .await
        .unwrap();
    assert!(
        pump_reaped.load(Ordering::SeqCst),
        "firmware session teardown must reap the registered pump"
    );
}

async fn connect_session(
    address: std::net::SocketAddr,
    task_set: FirmwareMqttTaskSet,
) -> FirmwareMqttSession {
    let mut options = MqttOptions::new(
        format!("firmware-parent-owner-{}", uuid::Uuid::new_v4()),
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(30)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    FirmwareMqttSession::connect_with_options_and_task_set(
        options,
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        task_set,
    )
    .await
    .unwrap()
}

fn test_options(address: std::net::SocketAddr) -> MqttOptions {
    let mut options = MqttOptions::new(
        format!("firmware-cancel-order-{}", uuid::Uuid::new_v4()),
        (address.ip().to_string(), address.port()),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(30)
        .set_max_packet_size(256 * 1024, 256 * 1024);
    options
}

async fn accept_subscription(stream: &mut TcpStream) {
    assert_eq!(read_packet(stream).await >> 4, 1);
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    assert_eq!(read_packet(stream).await >> 4, 8);
    stream
        .write_all(&[0x90, 0x03, 0x00, 0x01, 0x01])
        .await
        .unwrap();
}

async fn read_packet(stream: &mut TcpStream) -> u8 {
    let header = stream.read_u8().await.unwrap();
    let mut multiplier = 1usize;
    let mut remaining = 0usize;
    loop {
        let encoded = stream.read_u8().await.unwrap();
        remaining += usize::from(encoded & 0x7f) * multiplier;
        if encoded & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let mut body = vec![0; remaining];
    stream.read_exact(&mut body).await.unwrap();
    header
}
