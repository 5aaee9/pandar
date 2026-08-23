//! Account-scoped Studio printer-event WebSocket lifecycle.

use std::{
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use super::{ConnectionState, DispatcherWake, RequestSnapshot};

mod cache;
mod protocol;
#[cfg(test)]
mod tests;

pub(crate) const HEARTBEAT_BUSY_WAIT_MS: u32 = 0;

const OUTAGE_GRACE: Duration = Duration::from_secs(30);
const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const BACKOFF_STEPS_MS: [u64; 5] = [1_000, 2_000, 4_000, 8_000, 16_000];
const BACKOFF_CAP_MS: u64 = 30_000;

#[derive(Clone, Copy)]
pub(super) struct Fence {
    generation: u64,
    account_epoch: u64,
}

impl Fence {
    pub(super) fn of(config: &StreamConfig) -> Self {
        Self {
            generation: config.generation,
            account_epoch: config.account_epoch,
        }
    }

    pub(super) fn matches(self, state: &ConnectionState) -> bool {
        state.generation == self.generation && state.account_epoch == self.account_epoch
    }
}

pub(crate) struct StreamConfig {
    pub(crate) hub_url: String,
    pub(crate) url: String,
    pub(crate) token: String,
    pub(crate) generation: u64,
    pub(crate) account_epoch: u64,
}

impl From<&StreamConfig> for RequestSnapshot {
    fn from(config: &StreamConfig) -> Self {
        Self {
            hub_url: config.hub_url.clone(),
            generation: config.generation,
            account_epoch: config.account_epoch,
        }
    }
}

pub(super) struct StreamSignals {
    snapshot_committed: Mutex<u64>,
    changed: Condvar,
    async_committed: watch::Sender<u64>,
}

impl StreamSignals {
    pub(super) fn new() -> Self {
        Self {
            snapshot_committed: Mutex::new(0),
            changed: Condvar::new(),
            async_committed: watch::channel(0).0,
        }
    }

    pub(super) fn notify_snapshot(&self) {
        *self.snapshot_committed.lock().expect("stream signals") += 1;
        self.changed.notify_all();
        self.async_committed
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    fn subscribe_commits(&self) -> watch::Receiver<u64> {
        self.async_committed.subscribe()
    }

    pub(super) fn wait_for_snapshot(&self, timeout: Duration) {
        let guard = self.snapshot_committed.lock().expect("stream signals");
        let _ = self
            .changed
            .wait_timeout(guard, timeout.min(Duration::from_millis(100)))
            .expect("stream signals");
    }
}

pub(super) fn printer_events_url(hub_url: &str, tenant_id: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(hub_url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    {
        let mut segments = url.path_segments_mut().ok()?;
        segments.pop_if_empty();
        segments.extend(["api", "v1", "tenants", tenant_id.trim(), "printer-events"]);
    }
    url.set_query(Some("projection=studio&version=1"));
    url.set_fragment(None);
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme).ok()?;
    Some(url.into())
}

fn backoff_delay(attempt: usize) -> Duration {
    let ms = BACKOFF_STEPS_MS
        .get(attempt)
        .copied()
        .unwrap_or(BACKOFF_CAP_MS)
        .min(BACKOFF_CAP_MS);
    Duration::from_millis(ms)
}

struct Outage {
    next_health: Instant,
}

impl Outage {
    fn started_now() -> Self {
        Self {
            next_health: Instant::now() + OUTAGE_GRACE,
        }
    }

    fn mark_health_done(&mut self) {
        self.next_health = Instant::now() + HEALTH_INTERVAL;
    }
}

fn stream_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("plugin stream runtime can be created")
    })
}

pub(super) struct StreamWorker {
    cancel: CancellationToken,
    wake: Arc<Notify>,
    join: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl StreamWorker {
    pub(super) fn spawn(
        state: Arc<Mutex<ConnectionState>>,
        signals: Arc<StreamSignals>,
        dispatcher: Arc<Mutex<Option<DispatcherWake>>>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let wake = Arc::new(Notify::new());
        let join = stream_runtime().spawn(run_loop(
            state,
            signals,
            dispatcher,
            Arc::clone(&wake),
            cancel.clone(),
        ));
        Self {
            cancel,
            wake,
            join: Mutex::new(Some(join)),
        }
    }

    pub(super) fn wake(&self) {
        self.wake.notify_one();
    }

    pub(super) fn cancel_and_join(self) {
        self.cancel.cancel();
        self.wake.notify_one();
        if let Some(join) = self.join.lock().expect("stream worker handle").take()
            && let Err(error) = stream_runtime().block_on(join)
        {
            eprintln!("pandar printer event stream worker join failed: {error:#}");
        }
    }
}

fn notify_dispatcher(dispatcher: &Arc<Mutex<Option<DispatcherWake>>>) {
    if let Some(wake) = *dispatcher.lock().expect("dispatcher waker") {
        wake.notify();
    }
}

async fn wait_for_configuration(wake: &Notify, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = wake.notified() => true,
    }
}

fn generation_changed(state: &Arc<Mutex<ConnectionState>>, generation: u64) -> bool {
    state
        .lock()
        .expect("connection state")
        .stream_config()
        .is_none_or(|current| current.generation != generation)
}

async fn run_loop(
    state: Arc<Mutex<ConnectionState>>,
    signals: Arc<StreamSignals>,
    dispatcher: Arc<Mutex<Option<DispatcherWake>>>,
    wake: Arc<Notify>,
    cancel: CancellationToken,
) {
    let mut attempt = 0_usize;
    let mut last_generation = None;
    let mut outage: Option<Outage> = None;
    let mut commits = signals.subscribe_commits();

    loop {
        if cancel.is_cancelled() {
            break;
        }
        let config = state.lock().expect("connection state").stream_config();
        let Some(config) = config else {
            if !wait_for_configuration(&wake, &cancel).await {
                break;
            }
            continue;
        };
        if last_generation != Some(config.generation) {
            attempt = 0;
            outage = None;
            last_generation = Some(config.generation);
            commits.borrow_and_update();
        }
        outage.get_or_insert_with(Outage::started_now);

        let mut stream = std::pin::pin!(protocol::dial_and_stream(
            &state,
            &signals,
            &dispatcher,
            &config,
            &cancel,
        ));
        let outcome = loop {
            let health_deadline = outage.as_ref().map(|outage| outage.next_health);
            let health_wait = async {
                match health_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;

                _ = cancel.cancelled() => break protocol::StreamOutcome::Cancelled,
                _ = wake.notified() => {
                    if generation_changed(&state, config.generation) {
                        break protocol::StreamOutcome::Fenced;
                    }
                }
                changed = commits.changed() => {
                    if changed.is_ok() {
                        attempt = 0;
                        outage = None;
                    }
                }
                _ = health_wait => {
                    cache::observe_health(&state, RequestSnapshot::from(&config)).await;
                    notify_dispatcher(&dispatcher);
                    if let Some(outage) = outage.as_mut() {
                        outage.mark_health_done();
                    }
                }
                outcome = &mut stream => break outcome,
            }
        };

        match outcome {
            protocol::StreamOutcome::Cancelled => break,
            protocol::StreamOutcome::Fenced => continue,
            protocol::StreamOutcome::AuthRejected => {
                attempt = 0;
                outage = None;
                loop {
                    if !wait_for_configuration(&wake, &cancel).await {
                        return;
                    }
                    if generation_changed(&state, config.generation) {
                        break;
                    }
                }
                continue;
            }
            protocol::StreamOutcome::Failed { committed } => {
                if committed {
                    attempt = 0;
                    outage = None;
                }
                outage.get_or_insert_with(Outage::started_now);
            }
        }

        let retry_deadline = tokio::time::Instant::now() + backoff_delay(attempt);
        attempt += 1;
        loop {
            let health_deadline = outage.as_ref().map(|outage| outage.next_health);
            let health_wait = async {
                match health_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;

                _ = cancel.cancelled() => return,
                _ = wake.notified() => {
                    if generation_changed(&state, config.generation) {
                        break;
                    }
                }
                _ = health_wait => {
                    cache::observe_health(&state, RequestSnapshot::from(&config)).await;
                    notify_dispatcher(&dispatcher);
                    if let Some(outage) = outage.as_mut() {
                        outage.mark_health_done();
                    }
                }
                _ = tokio::time::sleep_until(retry_deadline) => break,
            }
        }
    }
}
