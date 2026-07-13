use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use tokio::{
    sync::Mutex,
    task::{AbortHandle, JoinHandle},
};

#[cfg(test)]
use super::FirmwareBarrierPause;

static NEXT_PUMP_TASK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub(crate) struct FirmwareMqttTaskSet {
    tasks: Arc<Mutex<HashMap<u64, OwnedPumpTask>>>,
}

struct OwnedPumpTask {
    task: JoinHandle<anyhow::Result<()>>,
    abort: FirmwarePumpAbortHandle,
    finished: Option<Arc<AtomicBool>>,
    reaped: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    join_pause: Option<FirmwareBarrierPause>,
}

pub(super) struct PumpOwner {
    task_id: u64,
    task_set: FirmwareMqttTaskSet,
    abort: FirmwarePumpAbortHandle,
}

#[derive(Clone)]
pub(crate) struct FirmwarePumpAbortHandle {
    task: AbortHandle,
    #[cfg(test)]
    requested: Arc<AtomicBool>,
}

impl FirmwarePumpAbortHandle {
    fn new(task: AbortHandle) -> Self {
        Self {
            task,
            #[cfg(test)]
            requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn abort(&self) {
        #[cfg(test)]
        self.requested.store(true, Ordering::SeqCst);
        self.task.abort();
    }

    #[cfg(test)]
    pub(super) fn requested_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.requested)
    }
}

impl FirmwareMqttTaskSet {
    pub(super) async fn spawn<F>(
        &self,
        future: F,
        finished: Option<Arc<AtomicBool>>,
        reaped: Option<Arc<AtomicBool>>,
    ) -> PumpOwner
    where
        F: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let task_id = NEXT_PUMP_TASK_ID.fetch_add(1, Ordering::Relaxed);
        let mut tasks = self.tasks.lock().await;
        let task = tokio::spawn(future);
        let abort = FirmwarePumpAbortHandle::new(task.abort_handle());
        let previous = tasks.insert(
            task_id,
            OwnedPumpTask {
                task,
                abort: abort.clone(),
                finished,
                reaped,
                #[cfg(test)]
                join_pause: None,
            },
        );
        assert!(previous.is_none(), "firmware MQTT pump task id is unique");
        PumpOwner {
            task_id,
            task_set: self.clone(),
            abort,
        }
    }

    pub(crate) async fn abort_and_join_all(&self) -> anyhow::Result<()> {
        let mut failure = None;
        loop {
            let task_id = self.tasks.lock().await.keys().next().copied();
            let Some(task_id) = task_id else {
                break;
            };
            let result = self.join_task(task_id, true).await;
            let error = match result {
                Ok(Ok(())) => None,
                Err(error) if error.is_cancelled() => None,
                Ok(Err(error)) => Some(error.context("run firmware MQTT pump during teardown")),
                Err(error) => Some(
                    anyhow::Error::new(error).context("join firmware MQTT pump during teardown"),
                ),
            };
            if let Some(error) = error {
                if failure.is_none() {
                    failure = Some(error);
                } else {
                    tracing::warn!(
                        error = %format!("{error:#}"),
                        "additional firmware MQTT pump teardown failure"
                    );
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn join_task(
        &self,
        task_id: u64,
        abort: bool,
    ) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
        let mut tasks = self.tasks.lock().await;
        let Some(task) = tasks.get_mut(&task_id) else {
            return Ok(Ok(()));
        };
        if abort {
            task.abort.abort();
        }
        #[cfg(test)]
        if let Some(pause) = task.join_pause.take() {
            let _ = pause.reached.send(());
            let _ = pause.release.await;
        }
        let result = (&mut task.task).await;
        let mut task = tasks
            .remove(&task_id)
            .expect("completed firmware MQTT pump remains registered");
        if let Some(finished) = task.finished.take() {
            finished.store(true, Ordering::SeqCst);
        }
        if let Some(reaped) = task.reaped.take() {
            reaped.store(true, Ordering::SeqCst);
        }
        result
    }

    #[cfg(test)]
    async fn pause_join(&self, task_id: u64, pause: FirmwareBarrierPause) {
        self.tasks
            .lock()
            .await
            .get_mut(&task_id)
            .expect("firmware MQTT pump owner has a registered task")
            .join_pause = Some(pause);
    }
}

impl PumpOwner {
    pub(super) fn abort_handle(&self) -> FirmwarePumpAbortHandle {
        self.abort.clone()
    }

    pub(super) async fn join(&mut self) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
        self.task_set.join_task(self.task_id, false).await
    }

    pub(super) async fn abort_and_join(
        &mut self,
    ) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
        self.task_set.join_task(self.task_id, true).await
    }

    #[cfg(test)]
    pub(super) async fn pause_join_for_test(&mut self, pause: FirmwareBarrierPause) {
        self.task_set.pause_join(self.task_id, pause).await;
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn abort_and_join_returns_completed_pump_run_error() {
        let task_set = FirmwareMqttTaskSet::default();
        let (started, wait_started) = oneshot::channel();
        let mut owner = task_set
            .spawn(
                async move {
                    let _ = started.send(());
                    Err(anyhow::anyhow!("completed pump run error sentinel"))
                },
                None,
                None,
            )
            .await;
        wait_started.await.unwrap();
        wait_owner_task_finished(&owner).await;

        let error = owner.abort_and_join().await.unwrap().unwrap_err();

        assert!(format!("{error:#}").contains("completed pump run error sentinel"));
    }

    #[tokio::test]
    async fn abort_and_join_returns_completed_pump_join_error() {
        let task_set = FirmwareMqttTaskSet::default();
        let (started, wait_started) = oneshot::channel();
        let mut owner = task_set
            .spawn(
                async move {
                    let _ = started.send(());
                    panic!("completed pump panic sentinel");
                },
                None,
                None,
            )
            .await;
        wait_started.await.unwrap();
        wait_owner_task_finished(&owner).await;

        let error = owner.abort_and_join().await.unwrap_err();

        assert!(!error.is_cancelled());
        assert!(format!("{error:#}").contains("completed pump panic sentinel"));
    }

    #[tokio::test]
    async fn global_and_owner_join_share_one_reap_without_panic_or_lost_error() {
        let task_set = FirmwareMqttTaskSet::default();
        let (started, wait_started) = oneshot::channel();
        let mut owner = task_set
            .spawn(
                async move {
                    let _ = started.send(());
                    Err(anyhow::anyhow!("concurrent pump error sentinel"))
                },
                None,
                None,
            )
            .await;
        wait_started.await.unwrap();
        wait_owner_task_finished(&owner).await;

        let (global, local) = tokio::join!(task_set.abort_and_join_all(), owner.abort_and_join());
        let messages = [
            global.err().map(|error| format!("{error:#}")),
            local
                .ok()
                .and_then(Result::err)
                .map(|error| format!("{error:#}")),
        ];

        assert_eq!(
            messages
                .iter()
                .filter(|message| message
                    .as_deref()
                    .is_some_and(|message| { message.contains("concurrent pump error sentinel") }))
                .count(),
            1,
            "exactly one join owner must report the pump error: {messages:?}"
        );
    }

    async fn wait_owner_task_finished(owner: &PumpOwner) {
        while !owner
            .task_set
            .tasks
            .lock()
            .await
            .get(&owner.task_id)
            .unwrap()
            .task
            .is_finished()
        {
            tokio::task::yield_now().await;
        }
    }
}
