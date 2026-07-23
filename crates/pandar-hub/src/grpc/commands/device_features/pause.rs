use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use tokio::sync::oneshot;

use crate::sessions::SessionToken;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Phase {
    AfterQueuedRowRead,
    AfterFeatureValidation,
    BeforeChannelSend,
}

struct PausePoint {
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

pub(crate) struct DispatchPause {
    reached: oneshot::Receiver<()>,
    resume: Option<oneshot::Sender<()>>,
}

pub(crate) fn install(token: SessionToken, phase: Phase) -> DispatchPause {
    let (reached_sender, reached_receiver) = oneshot::channel();
    let (resume_sender, resume_receiver) = oneshot::channel();
    let previous = pauses().lock().unwrap().insert(
        (token, phase),
        PausePoint {
            reached: reached_sender,
            resume: resume_receiver,
        },
    );
    assert!(previous.is_none(), "dispatch pause already installed");
    DispatchPause {
        reached: reached_receiver,
        resume: Some(resume_sender),
    }
}

impl DispatchPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
            .await
            .expect("timed out waiting for dispatch pause")
            .expect("dispatch pause was dropped before being reached");
    }

    pub(crate) fn resume(mut self) {
        let _ = self.resume.take().unwrap().send(());
    }
}

pub(super) async fn wait(token: SessionToken, phase: Phase) {
    let pause = pauses().lock().unwrap().remove(&(token, phase));
    if let Some(pause) = pause {
        let _ = pause.reached.send(());
        let _ = pause.resume.await;
    }
}

fn pauses() -> &'static Mutex<HashMap<(SessionToken, Phase), PausePoint>> {
    static PAUSES: OnceLock<Mutex<HashMap<(SessionToken, Phase), PausePoint>>> = OnceLock::new();
    PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}
