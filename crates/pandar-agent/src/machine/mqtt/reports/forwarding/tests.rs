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

use crate::machine::mqtt::{BambuMqttTransport, PublishedMqttCommand, mqtt_report_idle_timeout};
use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, DeviceFeatureCache, FirmwareObservationCache, FirmwareReportContext,
        RuntimeReportContext,
    },
};
use pandar_protocol::agent::v1::{AgentEvent, agent_event};

use super::{
    MqttForwardingContext, MqttPresenceState, forward_print_reports,
    forward_print_reports_with_context,
};

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
enum ControlledReport {
    Value(Value),
    IdleTimeout,
    TransportFailure,
}

#[derive(Debug)]
struct ControlledState {
    operations: StdMutex<Vec<ControlledOperation>>,
    report_receiver: Mutex<mpsc::UnboundedReceiver<ControlledReport>>,
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
    report_sender: mpsc::UnboundedSender<ControlledReport>,
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
            .send(ControlledReport::Value(report))
            .expect("controlled report receiver remains open");
    }

    fn push_idle_timeout(&self) {
        self.report_sender
            .send(ControlledReport::IdleTimeout)
            .expect("controlled report receiver remains open");
    }

    fn push_transport_failure(&self) {
        self.report_sender
            .send(ControlledReport::TransportFailure)
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

    async fn next_report(&self, timeout: Duration) -> anyhow::Result<Value> {
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
                Poll::Ready(Some(ControlledReport::Value(json!({
                    "unrelated": { "value": 1 }
                }))))
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
        match report {
            ControlledReport::Value(report) => Ok(report),
            ControlledReport::IdleTimeout => Err(mqtt_report_idle_timeout(timeout)),
            ControlledReport::TransportFailure => bail!("controlled MQTT transport failure"),
        }
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
    spawn_forwarder_for_endpoint(transport, endpoint())
}

fn spawn_forwarder_for_endpoint(
    transport: ControlledTransport,
    endpoint: BambuPrinterEndpoint,
) -> (JoinHandle<anyhow::Result<()>>, mpsc::Receiver<AgentEvent>) {
    let (sender, receiver) = mpsc::channel(128);
    let task_transport = transport.clone();
    let task = tokio::spawn(async move {
        forward_print_reports(
            &test_config(),
            &task_transport,
            &endpoint,
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

async fn next_snapshot(
    receiver: &mut mpsc::Receiver<AgentEvent>,
) -> pandar_protocol::agent::v1::PrinterSnapshot {
    loop {
        let event = next_event(receiver).await;
        if let Some(agent_event::Event::PrinterSnapshot(snapshot)) = event.event {
            return snapshot;
        }
    }
}

fn pushall_sequence_id(command: &PublishedMqttCommand) -> &str {
    command.payload["pushing"]["sequence_id"]
        .as_str()
        .expect("pushall sequence_id is a string")
}

mod authority;
mod refresh;
mod runtime_retry;
