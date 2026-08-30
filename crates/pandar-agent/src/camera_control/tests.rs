use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use tokio::{
    sync::{Mutex, mpsc},
    time::timeout,
};
use tonic::Status;

use super::*;
use crate::machine::{BambuPrinterEndpoint, NoopMachineGateway};
use pandar_protocol::agent::v1::{CloseCameraStream, OpenCameraStream};

#[tokio::test]
async fn firmware_camera_eof_drops_blocking_producer_before_return() {
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let (events, mut event_receiver) = mpsc::channel(2);

    forward_reverse_camera_session(
        &test_config(),
        "stream",
        &endpoint(),
        events,
        tokio_stream::empty(),
        move |_| producer,
    )
    .await
    .unwrap();

    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(matches!(
        event_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
}

#[tokio::test]
async fn firmware_camera_status_drops_blocking_producer_before_return() {
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let (events, mut event_receiver) = mpsc::channel(2);

    let error = forward_reverse_camera_session(
        &test_config(),
        "stream",
        &endpoint(),
        events,
        tokio_stream::iter([Err(Status::unavailable("camera stream status sentinel"))]),
        move |_| producer,
    )
    .await
    .unwrap_err();

    assert!(format!("{error:#}").contains("camera stream status sentinel"));
    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(matches!(
        event_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
}

#[tokio::test]
async fn firmware_camera_close_joins_blocking_producer_before_return() {
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let mut streams = HashMap::from([(
        "stream".into(),
        tokio::spawn(async move {
            let _ = producer.await;
        }),
    )]);

    timeout(
        Duration::from_millis(250),
        handle_control_camera_command(
            &test_config(),
            &NoopMachineGateway,
            &mut streams,
            close_command("stream"),
        ),
    )
    .await
    .expect("camera close must join the old outer task")
    .unwrap();

    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(streams.is_empty());
}

#[tokio::test]
async fn firmware_camera_replacement_joins_blocking_producer_before_return() {
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let mut streams = HashMap::from([(
        "stream".into(),
        tokio::spawn(async move {
            let _ = producer.await;
        }),
    )]);

    timeout(
        Duration::from_millis(250),
        handle_control_camera_command(
            &test_config(),
            &NoopMachineGateway,
            &mut streams,
            open_command("stream"),
        ),
    )
    .await
    .expect("camera replacement must join the old outer task")
    .unwrap();

    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    let replacement = streams.remove("stream").unwrap();
    replacement.abort();
    let _ = replacement.await;
}

#[tokio::test]
async fn firmware_camera_close_cancellation_keeps_task_owned_for_unified_teardown() {
    let stream_id = "cancel-close";
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let streams = Arc::new(Mutex::new(HashMap::from([(
        stream_id.into(),
        tokio::spawn(async move {
            let _ = producer.await;
        }),
    )])));
    let mut pause = install_camera_join_pause(stream_id);
    let worker_streams = Arc::clone(&streams);
    let worker = tokio::spawn(async move {
        let mut streams = worker_streams.lock().await;
        handle_control_camera_command(
            &test_config(),
            &NoopMachineGateway,
            &mut streams,
            close_command(stream_id),
        )
        .await
    });

    timeout(Duration::from_millis(250), pause.wait_reached())
        .await
        .expect("camera close must reach its join pause");
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());

    assert!(streams.lock().await.contains_key(stream_id));
    drop(pause);
    crate::command_stream::teardown_camera_tasks_for_test(&streams)
        .await
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(streams.lock().await.is_empty());
}

#[tokio::test]
async fn firmware_camera_open_cancellation_keeps_task_owned_for_unified_teardown() {
    let stream_id = "cancel-open";
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let streams = Arc::new(Mutex::new(HashMap::from([(
        stream_id.into(),
        tokio::spawn(async move {
            let _ = producer.await;
        }),
    )])));
    let mut pause = install_camera_join_pause(stream_id);
    let worker_streams = Arc::clone(&streams);
    let worker = tokio::spawn(async move {
        let mut streams = worker_streams.lock().await;
        handle_control_camera_command(
            &test_config(),
            &NoopMachineGateway,
            &mut streams,
            open_command(stream_id),
        )
        .await
    });

    timeout(Duration::from_millis(250), pause.wait_reached())
        .await
        .expect("camera open replacement must reach its join pause");
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());

    assert!(streams.lock().await.contains_key(stream_id));
    drop(pause);
    crate::command_stream::teardown_camera_tasks_for_test(&streams)
        .await
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(streams.lock().await.is_empty());
}

#[tokio::test]
async fn firmware_camera_cancelled_teardown_can_retry_and_join_owned_task() {
    let stream_id = "cancel-teardown";
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let streams = Arc::new(Mutex::new(HashMap::from([(
        stream_id.into(),
        tokio::spawn(async move {
            let _ = producer.await;
        }),
    )])));
    let mut pause = install_camera_join_pause(stream_id);
    let teardown_streams = Arc::clone(&streams);
    let teardown = tokio::spawn(async move {
        crate::command_stream::teardown_camera_tasks_for_test(&teardown_streams).await
    });

    timeout(Duration::from_millis(250), pause.wait_reached())
        .await
        .expect("camera teardown must reach its join pause");
    teardown.abort();
    assert!(teardown.await.unwrap_err().is_cancelled());

    assert!(streams.lock().await.contains_key(stream_id));
    drop(pause);
    crate::command_stream::teardown_camera_tasks_for_test(&streams)
        .await
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(streams.lock().await.is_empty());
}

#[tokio::test]
async fn firmware_camera_teardown_preserves_panic_and_reaps_remaining_tasks() {
    let panicked = tokio::spawn(async {
        panic!("camera task panic sentinel");
    });
    while !panicked.is_finished() {
        tokio::task::yield_now().await;
    }
    let (producer, dropped, mut late_receiver) = blocking_producer();
    let streams = Mutex::new(HashMap::from([
        ("panic".into(), panicked),
        (
            "blocking".into(),
            tokio::spawn(async move {
                let _ = producer.await;
            }),
        ),
    ]));

    let error = crate::command_stream::teardown_camera_tasks_for_test(&streams)
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains("camera task panic sentinel"));
    assert!(dropped.load(Ordering::SeqCst));
    assert!(matches!(
        late_receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert!(streams.lock().await.is_empty());
}

fn blocking_producer() -> (BlockingProducer, Arc<AtomicBool>, mpsc::Receiver<()>) {
    let dropped = Arc::new(AtomicBool::new(false));
    let (late_sender, late_receiver) = mpsc::channel(1);
    (
        BlockingProducer {
            dropped: Arc::clone(&dropped),
            _late_sender: late_sender,
        },
        dropped,
        late_receiver,
    )
}

struct BlockingProducer {
    dropped: Arc<AtomicBool>,
    _late_sender: mpsc::Sender<()>,
}

impl Future for BlockingProducer {
    type Output = anyhow::Result<()>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

impl Drop for BlockingProducer {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

fn close_command(stream_id: &str) -> HubCameraCommand {
    HubCameraCommand {
        stream_id: stream_id.into(),
        command: Some(hub_camera_command::Command::Close(CloseCameraStream {})),
    }
}

fn open_command(stream_id: &str) -> HubCameraCommand {
    HubCameraCommand {
        stream_id: stream_id.into(),
        command: Some(hub_camera_command::Command::Open(OpenCameraStream {
            serial_number: "SERIAL".into(),
            mode: CameraStreamMode::Mjpeg as i32,
        })),
    }
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
        hub_grpc_url: "http://127.0.0.1:1".into(),
        hub_api_url: None,
        agent_name: "test".into(),
        agent_id: "agent".into(),
        tenant_id: "tenant".into(),
        agent_credential: "credential".into(),
        agent_version: "test".into(),
        printers: "[]".into(),
    }
}
