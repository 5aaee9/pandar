use std::{
    collections::VecDeque,
    future::pending,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use rumqttc::{Event, QoS};
use tokio::sync::Notify;

use crate::machine::mqtt::recovery::RecoveryAttempt;

pub(super) enum PollStep {
    Event(Event),
    Error(&'static str),
    Pending,
}

pub(super) struct FakeRecoveryAttempt {
    state: Arc<FakeRecoveryState>,
    steps: VecDeque<PollStep>,
    pub(super) publish_error: Option<&'static str>,
}

pub(super) struct FakeRecoveryProbe {
    state: Arc<FakeRecoveryState>,
}

struct FakeRecoveryState {
    publishes: Mutex<Vec<CapturedPublish>>,
    publish_calls: AtomicUsize,
    poll_calls: AtomicUsize,
    dropped: AtomicBool,
    unpolled_events_on_drop: AtomicUsize,
    poll_started: Arc<Notify>,
}

#[derive(Debug, Clone)]
pub(super) struct CapturedPublish {
    pub(super) topic: String,
    pub(super) qos: QoS,
    pub(super) retain: bool,
    pub(super) payload: Vec<u8>,
}

impl FakeRecoveryAttempt {
    pub(super) fn new(steps: impl IntoIterator<Item = PollStep>) -> (Self, FakeRecoveryProbe) {
        let state = Arc::new(FakeRecoveryState {
            publishes: Mutex::new(Vec::new()),
            publish_calls: AtomicUsize::new(0),
            poll_calls: AtomicUsize::new(0),
            dropped: AtomicBool::new(false),
            unpolled_events_on_drop: AtomicUsize::new(0),
            poll_started: Arc::new(Notify::new()),
        });
        (
            Self {
                state: state.clone(),
                steps: steps.into_iter().collect(),
                publish_error: None,
            },
            FakeRecoveryProbe { state },
        )
    }
}

impl Drop for FakeRecoveryAttempt {
    fn drop(&mut self) {
        self.state
            .unpolled_events_on_drop
            .store(self.steps.len(), Ordering::SeqCst);
        self.state.dropped.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl RecoveryAttempt for FakeRecoveryAttempt {
    async fn publish(
        &mut self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.state.publish_calls.fetch_add(1, Ordering::SeqCst);
        self.state.publishes.lock().unwrap().push(CapturedPublish {
            topic,
            qos,
            retain,
            payload,
        });
        if let Some(error) = self.publish_error {
            bail!(error);
        }
        Ok(())
    }

    async fn poll(&mut self) -> anyhow::Result<Event> {
        self.state.poll_calls.fetch_add(1, Ordering::SeqCst);
        self.state.poll_started.notify_one();
        match self.steps.pop_front().unwrap_or(PollStep::Pending) {
            PollStep::Event(event) => Ok(event),
            PollStep::Error(error) => Err(anyhow!(error)),
            PollStep::Pending => pending().await,
        }
    }
}

impl FakeRecoveryProbe {
    pub(super) fn publishes(&self) -> Vec<CapturedPublish> {
        self.state.publishes.lock().unwrap().clone()
    }

    pub(super) fn publish_calls(&self) -> usize {
        self.state.publish_calls.load(Ordering::SeqCst)
    }

    pub(super) fn poll_calls(&self) -> usize {
        self.state.poll_calls.load(Ordering::SeqCst)
    }

    pub(super) fn was_dropped(&self) -> bool {
        self.state.dropped.load(Ordering::SeqCst)
    }

    pub(super) fn unpolled_events_on_drop(&self) -> usize {
        self.state.unpolled_events_on_drop.load(Ordering::SeqCst)
    }

    pub(super) fn poll_started(&self) -> Arc<Notify> {
        self.state.poll_started.clone()
    }
}
