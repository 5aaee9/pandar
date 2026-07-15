use std::{
    future::poll_fn,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;
use pandar_core::BambuDeviceFeatures;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, Notify, mpsc},
    task::JoinHandle,
    task::yield_now,
    time::advance,
};

use crate::machine::mqtt::{BambuMqttTransport, PublishedMqttCommand};
use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, DeviceFeatureCache, FirmwareObservationCache, FirmwareReportContext,
        RuntimeReportContext,
    },
    protocol::agent::v1::{AgentEvent, agent_event},
};

use super::forward_print_reports;

const EXPECTED_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const SETTLE_ATTEMPTS: usize = 512;

#[derive(Debug, Clone)]
enum ControlledOperation {
    Subscribe(String),
    Publish {
        ordinal: usize,
        command: PublishedMqttCommand,
    },
    ReportWaitArmed(usize),
    ReportDelivered(usize),
}

#[derive(Debug)]
struct ControlledState {
    operations: StdMutex<Vec<ControlledOperation>>,
    report_receiver: Mutex<mpsc::UnboundedReceiver<Value>>,
    publish_attempts: AtomicUsize,
    report_waits_armed: AtomicUsize,
    reports_delivered: AtomicUsize,
    ready_reports: AtomicUsize,
    armed_notify: Notify,
    fail_publish_attempt: Option<usize>,
}

#[derive(Debug, Clone)]
struct ControlledTransport {
    state: Arc<ControlledState>,
    report_sender: mpsc::UnboundedSender<Value>,
}

impl ControlledTransport {
    fn new(fail_publish_attempt: Option<usize>) -> Self {
        let (report_sender, report_receiver) = mpsc::unbounded_channel();
        Self {
            state: Arc::new(ControlledState {
                operations: StdMutex::new(Vec::new()),
                report_receiver: Mutex::new(report_receiver),
                publish_attempts: AtomicUsize::new(0),
                report_waits_armed: AtomicUsize::new(0),
                reports_delivered: AtomicUsize::new(0),
                ready_reports: AtomicUsize::new(0),
                armed_notify: Notify::new(),
                fail_publish_attempt,
            }),
            report_sender,
        }
    }

    fn push_report(&self, report: Value) {
        self.report_sender
            .send(report)
            .expect("controlled report receiver remains open");
    }
    fn make_reports_ready_without_waking(&self, count: usize) {
        self.state.ready_reports.store(count, Ordering::SeqCst);
    }

    fn operations(&self) -> Vec<ControlledOperation> {
        self.state.operations.lock().unwrap().clone()
    }

    fn published_commands(&self) -> Vec<(usize, PublishedMqttCommand)> {
        self.operations()
            .into_iter()
            .filter_map(|operation| match operation {
                ControlledOperation::Publish { ordinal, command } => Some((ordinal, command)),
                _ => None,
            })
            .collect()
    }

    fn publish_attempts(&self) -> usize {
        self.state.publish_attempts.load(Ordering::SeqCst)
    }

    fn report_waits_armed(&self) -> usize {
        self.state.report_waits_armed.load(Ordering::SeqCst)
    }

    fn reports_delivered(&self) -> usize {
        self.state.reports_delivered.load(Ordering::SeqCst)
    }

    fn subscription_count(&self) -> usize {
        self.operations()
            .into_iter()
            .filter(|operation| matches!(operation, ControlledOperation::Subscribe(_)))
            .count()
    }

    async fn wait_for_publish_attempts(&self, expected: usize) {
        settle_until(
            || self.publish_attempts() >= expected,
            "expected controlled MQTT publish attempt was not observed",
        )
        .await;
    }

    async fn wait_for_report_waits(&self, expected: usize) {
        for _ in 0..SETTLE_ATTEMPTS {
            if self.report_waits_armed() >= expected {
                return;
            }
            let notified = self.state.armed_notify.notified();
            if self.report_waits_armed() >= expected {
                return;
            }
            tokio::select! {
                biased;
                _ = notified => {}
                _ = yield_now() => {}
            }
        }
        assert!(
            self.report_waits_armed() >= expected,
            "expected controlled report wait {expected}, observed {}",
            self.report_waits_armed()
        );
    }

    async fn wait_for_deliveries(&self, expected: usize) {
        settle_until(
            || self.reports_delivered() >= expected,
            "expected controlled MQTT report delivery was not observed",
        )
        .await;
    }

    async fn wait_for_subscriptions(&self, expected: usize) {
        settle_until(
            || self.subscription_count() >= expected,
            "expected controlled MQTT subscription was not observed",
        )
        .await;
    }
}

#[async_trait]
impl BambuMqttTransport for ControlledTransport {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        self.state
            .operations
            .lock()
            .unwrap()
            .push(ControlledOperation::Subscribe(topic.to_owned()));
        Ok(())
    }

    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()> {
        let ordinal = self.state.publish_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .operations
            .lock()
            .unwrap()
            .push(ControlledOperation::Publish { ordinal, command });
        if self.state.fail_publish_attempt == Some(ordinal) {
            bail!("controlled MQTT publish failure at attempt {ordinal}");
        }
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<Value> {
        let mut receiver = self.state.report_receiver.lock().await;
        let armed = self.state.report_waits_armed.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .operations
            .lock()
            .unwrap()
            .push(ControlledOperation::ReportWaitArmed(armed));
        self.state.armed_notify.notify_waiters();
        let report = poll_fn(|context| {
            if self
                .state
                .ready_reports
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |ready| {
                    if ready > 0 { Some(ready - 1) } else { None }
                })
                .is_ok()
            {
                Poll::Ready(Some(json!({ "unrelated": { "value": 1 } })))
            } else {
                receiver.poll_recv(context)
            }
        })
        .await
        .ok_or_else(|| anyhow::anyhow!("controlled MQTT report source closed"))?;
        let delivered = self.state.reports_delivered.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .operations
            .lock()
            .unwrap()
            .push(ControlledOperation::ReportDelivered(delivered));
        Ok(report)
    }
}

async fn settle_until(mut predicate: impl FnMut() -> bool, message: &str) {
    for _ in 0..SETTLE_ATTEMPTS {
        if predicate() {
            return;
        }
        yield_now().await;
    }
    assert!(predicate(), "{message}");
}

fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    }
}

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_owned(),
        serial: "01S00EXAMPLE".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some("A1 Mini".to_owned()),
        name: Some("garage-a1".to_owned()),
    }
}

fn spawn_forwarder(
    transport: ControlledTransport,
) -> (JoinHandle<anyhow::Result<()>>, mpsc::Receiver<AgentEvent>) {
    let (sender, receiver) = mpsc::channel(128);
    let task_transport = transport.clone();
    let task = tokio::spawn(async move {
        forward_print_reports(
            &test_config(),
            &task_transport,
            &endpoint(),
            Duration::from_secs(10),
            &sender,
            &DeviceFeatureCache::default(),
        )
        .await
    });
    (task, receiver)
}

async fn abort_and_join(task: JoinHandle<anyhow::Result<()>>) {
    task.abort();
    let error = task
        .await
        .expect_err("aborted forwarding task must return a join error");
    assert!(error.is_cancelled());
}

async fn wait_for_task_finish(task: &JoinHandle<anyhow::Result<()>>) {
    settle_until(
        || task.is_finished(),
        "expected forwarding task to finish cooperatively",
    )
    .await;
}

async fn next_event(receiver: &mut mpsc::Receiver<AgentEvent>) -> AgentEvent {
    for _ in 0..SETTLE_ATTEMPTS {
        if let Ok(event) = receiver.try_recv() {
            return event;
        }
        yield_now().await;
    }
    receiver
        .try_recv()
        .expect("expected Agent event was not observed")
}

fn pushall_sequence_id(command: &PublishedMqttCommand) -> &str {
    command.payload["pushing"]["sequence_id"]
        .as_str()
        .expect("pushall sequence_id is a string")
}

#[test]
fn periodic_printer_refresh_uses_exact_sixty_second_constant() {
    assert_eq!(super::PRINTER_REFRESH_INTERVAL, Duration::from_secs(60));
}
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

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_runtime_failure_retries_with_fresh_timer() {
    let config = test_config();
    let endpoint = endpoint();
    let firmware_cache = FirmwareObservationCache::default();
    let device_features = DeviceFeatureCache::default();
    device_features
        .update(&endpoint.serial, BambuDeviceFeatures::from_bits(0x40))
        .await;
    let (sender, mut receiver) = mpsc::channel(128);
    let transition = firmware_cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    let initial_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterFirmwareInvalidated(initial)) = initial_event.event else {
        panic!("initial generation must emit firmware invalidation");
    };
    assert_eq!(initial.serial, endpoint.serial);
    assert_eq!(initial.generation, generation_one);

    let transport = ControlledTransport::new(Some(3));
    let task_transport = transport.clone();
    let task_config = config.clone();
    let task_endpoint = endpoint.clone();
    let task_firmware_cache = firmware_cache.clone();
    let task_device_features = device_features.clone();
    let task = tokio::spawn(async move {
        crate::machine::runtime::forward_print_reports_with_firmware_retry(
            task_config,
            task_transport,
            task_endpoint,
            Duration::from_secs(10),
            sender,
            Duration::from_secs(5),
            RuntimeReportContext {
                device_features: task_device_features,
                firmware: FirmwareReportContext {
                    cache: task_firmware_cache,
                    generation: generation_one,
                },
            },
        )
        .await;
    });

    transport.wait_for_subscriptions(1).await;
    transport.wait_for_publish_attempts(2).await;
    transport.wait_for_report_waits(1).await;
    let initial_publishes = transport.published_commands();
    assert_eq!(initial_publishes[0].0, 1);
    assert_eq!(
        initial_publishes[0].1.payload["info"]["command"],
        "get_version"
    );
    assert_eq!(initial_publishes[1].0, 2);
    assert_eq!(
        initial_publishes[1].1.payload["pushing"]["command"],
        "pushall"
    );

    advance(Duration::from_secs(60)).await;
    transport.wait_for_publish_attempts(3).await;
    let failed_publish = &transport.published_commands()[2];
    assert_eq!(failed_publish.0, 3);
    assert_eq!(failed_publish.1.payload["pushing"]["command"], "pushall");
    let invalidation_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterFirmwareInvalidated(invalidation)) =
        invalidation_event.event
    else {
        panic!("periodic failure must invalidate the firmware generation");
    };
    assert_eq!(invalidation.serial, endpoint.serial);
    assert_eq!(invalidation.generation, generation_one + 1);
    let generation_two = invalidation.generation;

    let feature_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(features)) = feature_event.event
    else {
        panic!("periodic failure must invalidate cached device features");
    };
    assert_eq!(features.serial, endpoint.serial);
    assert!(features.device_features.is_none());
    assert!(device_features.get(&endpoint.serial).await.is_none());
    assert_eq!(
        firmware_cache
            .snapshot(&endpoint.serial)
            .await
            .expect("new firmware generation remains active")
            .generation,
        generation_two
    );

    advance(Duration::from_secs(5) - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.subscription_count(), 1);
    assert_eq!(transport.publish_attempts(), 3);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_subscriptions(2).await;
    transport.wait_for_publish_attempts(5).await;
    transport.wait_for_report_waits(2).await;

    let retry_publishes = transport.published_commands();
    assert_eq!(retry_publishes[3].0, 4);
    assert_eq!(
        retry_publishes[3].1.payload["info"]["command"],
        "get_version"
    );
    assert_eq!(retry_publishes[4].0, 5);
    assert_eq!(
        retry_publishes[4].1.payload["pushing"]["command"],
        "pushall"
    );

    advance(Duration::from_secs(55)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 5);
    advance(Duration::from_secs(5) - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 5);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_publish_attempts(6).await;
    transport.wait_for_report_waits(3).await;
    let final_publishes = transport.published_commands();
    assert_eq!(final_publishes[5].0, 6);
    assert_eq!(
        final_publishes[5].1.payload["pushing"]["command"],
        "pushall"
    );
    assert_eq!(transport.subscription_count(), 2);

    drop(receiver);
    settle_until(
        || task.is_finished(),
        "retry wrapper must stop when the Agent event receiver closes",
    )
    .await;
    task.await.unwrap();
}
