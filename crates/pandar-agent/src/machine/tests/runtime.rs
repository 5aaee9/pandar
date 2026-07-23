use super::*;

mod device_features_replacement;
mod device_features_startup;
mod firmware;
mod gateway;
mod install;

const DEVICE_FEATURE_HIGH_BITS: u64 = 0x8000_0041_0000_0020;

fn feature_event_bits(event: crate::protocol::agent::v1::AgentEvent) -> Option<u64> {
    let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) = event.event else {
        panic!("expected printer device features event, got {event:?}");
    };
    snapshot
        .device_features
        .map(|features| features.bambu_fun_bits)
}

fn assert_offline_event(event: crate::protocol::agent::v1::AgentEvent) {
    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = event.event else {
        panic!("expected offline printer snapshot, got {event:?}");
    };
    assert_eq!(snapshot.state, "offline");
    assert!(!snapshot.telemetry_authoritative);
}

struct StaleReportTaskFinished(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for StaleReportTaskFinished {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

async fn install_stale_report_cache_write(
    report_tasks: &tokio::sync::Mutex<
        std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
    >,
    cache: crate::machine::DeviceFeatureCache,
    serial: &str,
    value: BambuDeviceFeatures,
    release: std::sync::Arc<Notify>,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let serial = serial.to_owned();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let task_finished = std::sync::Arc::clone(&finished);
    let (started, running) = tokio::sync::oneshot::channel();
    report_tasks.lock().await.insert(
        serial.clone(),
        tokio::spawn(async move {
            let _finished = StaleReportTaskFinished(task_finished);
            started.send(()).unwrap();
            release.notified().await;
            cache.update(&serial, value).await;
        }),
    );
    running.await.unwrap();
    finished
}

#[derive(Clone)]
struct PausedMqttTransport {
    state: std::sync::Arc<PausedMqttTransportState>,
}

struct PausedMqttTransportState {
    blocked: Notify,
    release: Notify,
    reports: Mutex<Vec<serde_json::Value>>,
    pause_first_report: bool,
}

impl PausedMqttTransport {
    fn new() -> Self {
        Self {
            state: std::sync::Arc::new(PausedMqttTransportState {
                blocked: Notify::new(),
                release: Notify::new(),
                reports: Mutex::new(vec![
                    get_version_report("X1 Carbon"),
                    runtime_state_report("READY"),
                ]),
                pause_first_report: true,
            }),
        }
    }

    fn ready(model: &str, state: &str) -> Self {
        Self {
            state: std::sync::Arc::new(PausedMqttTransportState {
                blocked: Notify::new(),
                release: Notify::new(),
                reports: Mutex::new(vec![get_version_report(model), runtime_state_report(state)]),
                pause_first_report: false,
            }),
        }
    }

    fn new_with_feature(fun: &'static str) -> Self {
        Self {
            state: std::sync::Arc::new(PausedMqttTransportState {
                blocked: Notify::new(),
                release: Notify::new(),
                reports: Mutex::new(vec![
                    get_version_report("X1 Carbon"),
                    runtime_feature_report("READY", fun),
                ]),
                pause_first_report: true,
            }),
        }
    }

    async fn wait_until_blocked(&self) {
        self.state.blocked.notified().await;
    }

    fn release(&self) {
        self.state.release.notify_waiters();
    }
}

#[async_trait]
impl BambuMqttTransport for PausedMqttTransport {
    async fn subscribe(&self, _topic: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn publish(&self, _command: PublishedMqttCommand) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<serde_json::Value> {
        if self.state.pause_first_report {
            let mut reports = self.state.reports.lock().await;
            if reports.len() == 2 {
                self.state.blocked.notify_one();
                drop(reports);
                self.state.release.notified().await;
                reports = self.state.reports.lock().await;
            }
            return Ok(reports.remove(0));
        }
        Ok(self.state.reports.lock().await.remove(0))
    }
}
