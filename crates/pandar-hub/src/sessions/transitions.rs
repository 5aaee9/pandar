use std::{collections::HashMap, sync::Arc};

use pandar_core::AgentId;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Debug, Clone, Default)]
pub(super) struct AgentTransitions {
    leases: Arc<Mutex<HashMap<AgentId, Arc<Mutex<()>>>>>,
}

impl AgentTransitions {
    pub(super) async fn lease(&self, agent_id: AgentId) -> OwnedMutexGuard<()> {
        self.mutex(agent_id).await.lock_owned().await
    }

    async fn mutex(&self, agent_id: AgentId) -> Arc<Mutex<()>> {
        self.leases
            .lock()
            .await
            .entry(agent_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    #[cfg(test)]
    pub(super) async fn lease_observed(
        &self,
        agent_id: AgentId,
        token: super::SessionToken,
    ) -> OwnedMutexGuard<()> {
        let lease = self.mutex(agent_id).await;
        match lease.clone().try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                pause::wait_waiting(token).await;
                lease.lock_owned().await
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tokio::sync::oneshot;

    use super::super::SessionToken;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Phase {
        Before,
        Waiting,
        After,
    }

    struct PausePoint {
        reached: oneshot::Sender<()>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct TransitionPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) struct TransitionWait {
        reached: oneshot::Receiver<()>,
    }

    pub(crate) fn install_before(token: SessionToken) -> TransitionPause {
        install(token, Phase::Before)
    }

    pub(crate) fn install_after(token: SessionToken) -> TransitionPause {
        install(token, Phase::After)
    }

    pub(crate) fn observe_waiting(token: SessionToken) -> TransitionWait {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        drop(resume_sender);
        let previous = pauses()
            .lock()
            .expect("transition pause mutex should not be poisoned")
            .insert(
                (token, Phase::Waiting),
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(previous.is_none(), "transition wait already installed");
        TransitionWait {
            reached: reached_receiver,
        }
    }

    fn install(token: SessionToken, phase: Phase) -> TransitionPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("transition pause mutex should not be poisoned")
            .insert(
                (token, phase),
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(previous.is_none(), "transition pause already installed");
        TransitionPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl TransitionPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for transition pause")
                .expect("transition pause was dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("transition pause resume sender must be present")
                .send(());
        }
    }

    impl TransitionWait {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for transition lock attempt")
                .expect("transition lock attempt was dropped before being reached");
        }
    }

    pub(crate) async fn wait_before(token: SessionToken) {
        wait(token, Phase::Before).await;
    }

    pub(crate) async fn wait_after(token: SessionToken) {
        wait(token, Phase::After).await;
    }

    pub(crate) async fn wait_waiting(token: SessionToken) {
        wait(token, Phase::Waiting).await;
    }

    async fn wait(token: SessionToken, phase: Phase) {
        let pause = pauses()
            .lock()
            .expect("transition pause mutex should not be poisoned")
            .remove(&(token, phase));
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<(SessionToken, Phase), PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<(SessionToken, Phase), PausePoint>>> =
            OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
