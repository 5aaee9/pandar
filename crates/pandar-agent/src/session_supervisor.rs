use anyhow::Context;

use crate::backoff::RunOutcome;

pub(super) struct SessionSupervisor {
    cancellation: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<anyhow::Result<RunOutcome>>>,
}

impl SessionSupervisor {
    pub(super) fn new(
        cancellation: tokio::sync::oneshot::Sender<()>,
        task: tokio::task::JoinHandle<anyhow::Result<RunOutcome>>,
    ) -> Self {
        Self {
            cancellation: Some(cancellation),
            task: Some(task),
        }
    }

    pub(super) async fn join(mut self) -> anyhow::Result<RunOutcome> {
        let result = self
            .task
            .as_mut()
            .expect("session supervisor has a task")
            .await;
        self.task = None;
        self.cancellation = None;
        result.context("join Agent reverse-session supervisor")?
    }
}

impl Drop for SessionSupervisor {
    fn drop(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            let _ = cancellation.send(());
        }
        let Some(task) = self.task.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(reap_session_task(task));
            }
            Err(_) => task.abort(),
        }
    }
}

pub(super) async fn reap_session_task(task: tokio::task::JoinHandle<anyhow::Result<RunOutcome>>) {
    match task.await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(
            error = %format!("{error:#}"),
            "cancelled Agent reverse-session supervisor failed"
        ),
        Err(error) if error.is_cancelled() => {}
        Err(error) => tracing::warn!(
            error = %format!("{error:#}"),
            "join cancelled Agent reverse-session supervisor"
        ),
    }
}
