use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use pandar_core::CommandId;
use tokio::sync::oneshot;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

struct PausePoint {
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

pub(crate) struct ExecuteOwnershipGapPause {
    reached: oneshot::Receiver<()>,
    resume: Option<oneshot::Sender<()>>,
}

pub(crate) fn install(command_id: CommandId) -> ExecuteOwnershipGapPause {
    let (reached_sender, reached_receiver) = oneshot::channel();
    let (resume_sender, resume_receiver) = oneshot::channel();
    let previous = pauses()
        .lock()
        .expect("execute ownership gap pause mutex should not be poisoned")
        .insert(
            command_id,
            PausePoint {
                reached: reached_sender,
                resume: resume_receiver,
            },
        );
    assert!(
        previous.is_none(),
        "execute ownership gap pause already installed"
    );
    ExecuteOwnershipGapPause {
        reached: reached_receiver,
        resume: Some(resume_sender),
    }
}

impl ExecuteOwnershipGapPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
            .await
            .expect("timed out waiting for execute ownership gap pause")
            .expect("execute ownership gap pause was dropped before being reached");
    }

    pub(crate) fn resume(mut self) {
        let _ = self
            .resume
            .take()
            .expect("execute ownership gap resume sender must be present")
            .send(());
    }
}

pub(crate) async fn wait(command_id: CommandId) {
    let pause = pauses()
        .lock()
        .expect("execute ownership gap pause mutex should not be poisoned")
        .remove(&command_id);
    if let Some(pause) = pause {
        let _ = pause.reached.send(());
        let _ = pause.resume.await;
    }
}

fn pauses() -> &'static Mutex<HashMap<CommandId, PausePoint>> {
    static PAUSES: OnceLock<Mutex<HashMap<CommandId, PausePoint>>> = OnceLock::new();
    PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}
