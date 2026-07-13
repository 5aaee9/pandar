use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::*;

#[tokio::test]
async fn firmware_cancelled_connect_waiting_to_register_never_spawns_an_unowned_pump() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (check_second, wait_check_second) = oneshot::channel();
    let (second_started, wait_second_started) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        accept_subscription(&mut first).await;
        wait_check_second.await.unwrap();
        let second = timeout(Duration::from_millis(250), listener.accept()).await;
        let mut second = match second {
            Ok(Ok((mut second, _))) => {
                accept_subscription(&mut second).await;
                second_started.send(true).unwrap();
                Some(second)
            }
            Err(_) => {
                second_started.send(false).unwrap();
                None
            }
            Ok(Err(error)) => panic!("accept second firmware MQTT connection: {error:#}"),
        };

        let packet = read_packet(&mut first).await;
        assert_eq!(packet.header >> 4, 14);
        assert_socket_closed(&mut first).await;
        if let Some(second) = second.as_mut() {
            assert_socket_closed(second).await;
        }
    });
    let task_set = FirmwareMqttTaskSet::default();
    let mut first = FirmwareMqttSession::connect_with_options_and_task_set(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        task_set.clone(),
    )
    .await
    .unwrap();
    let mut join_pause = first.pause_pump_join_for_test().await;
    let first_shutdown = tokio::spawn(async move { first.shutdown().await });
    join_pause.wait_until_reached().await;

    let second_finished = Arc::new(AtomicBool::new(false));
    let second_reaped = Arc::new(AtomicBool::new(false));
    let registration_waiting = Arc::new(AtomicBool::new(false));
    let second_connect = tokio::spawn(
        FirmwareMqttSession::connect_with_options_and_pump_guards_and_task_set(
            test_options(address),
            REQUEST_TOPIC.into(),
            REPORT_TOPIC.into(),
            Arc::clone(&second_finished),
            Arc::clone(&second_reaped),
            Arc::clone(&registration_waiting),
            task_set.clone(),
        ),
    );
    wait_finished(&registration_waiting).await;
    check_second.send(()).unwrap();
    let second_started = wait_second_started.await.unwrap();

    second_connect.abort();
    let _ = second_connect.await;
    join_pause.release();
    first_shutdown.await.unwrap().unwrap();
    task_set.abort_and_join_all().await.unwrap();

    assert!(!second_started, "pump started before task-set ownership");
    assert!(!second_finished.load(Ordering::SeqCst));
    assert!(!second_reaped.load(Ordering::SeqCst));
    timeout(Duration::from_millis(250), broker)
        .await
        .expect("cancelled registration wait must close its pump socket")
        .unwrap();
}

#[tokio::test]
async fn firmware_rejected_suback_preserves_completed_pump_error_cause() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (close_broker, wait_close_broker) = oneshot::channel();
    let broker = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_packet(&mut stream).await.header >> 4, 1);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        let subscribe = read_packet(&mut stream).await;
        assert_eq!(subscribe.header >> 4, 8);
        let packet_id = u16::from_be_bytes([subscribe.body[0], subscribe.body[1]]);
        stream
            .write_all(&[0x90, 0x03, (packet_id >> 8) as u8, packet_id as u8, 0x80])
            .await
            .unwrap();
        wait_close_broker.await.unwrap();
        drop(stream);
    });
    let task_set = FirmwareMqttTaskSet::default();
    let finished = Arc::new(AtomicBool::new(false));
    let (cleanup_pause, mut cleanup_pause_handle) = firmware_barrier_pause();
    let connect = tokio::spawn(FirmwareMqttSession::connect_with_options_and_cleanup_pause(
        test_options(address),
        REQUEST_TOPIC.into(),
        REPORT_TOPIC.into(),
        Arc::clone(&finished),
        cleanup_pause,
        task_set.clone(),
    ));
    cleanup_pause_handle.wait_until_reached().await;

    close_broker.send(()).unwrap();
    broker.await.unwrap();
    wait_finished(&finished).await;
    cleanup_pause_handle.release();

    let error = match connect.await.unwrap() {
        Ok(_) => panic!("rejected firmware SUBACK must fail the connection"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains("firmware MQTT subscription was rejected"));
    assert!(message.contains("poll firmware MQTT event loop"));
    task_set.abort_and_join_all().await.unwrap();
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

fn test_options(address: std::net::SocketAddr) -> MqttOptions {
    let mut options = MqttOptions::new(
        format!("firmware-registration-test-{}", uuid::Uuid::new_v4()),
        address.ip().to_string(),
        address.port(),
    );
    options
        .set_clean_session(true)
        .set_keep_alive(Duration::from_secs(30))
        .set_max_packet_size(256 * 1024, 256 * 1024);
    options
}
