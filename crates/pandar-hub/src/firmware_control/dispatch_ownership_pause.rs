use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use tokio::sync::oneshot;

struct PausePoint {
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

pub(crate) struct DispatchOwnershipPause {
    reached: oneshot::Receiver<()>,
    resume: Option<oneshot::Sender<()>>,
}

pub(crate) fn install(operation: &str, printer_id: &str) -> DispatchOwnershipPause {
    let (reached_sender, reached_receiver) = oneshot::channel();
    let (resume_sender, resume_receiver) = oneshot::channel();
    let key = pause_key(operation, printer_id);
    let previous = pauses()
        .lock()
        .expect("firmware dispatch ownership pause mutex should not be poisoned")
        .insert(
            key,
            PausePoint {
                reached: reached_sender,
                resume: resume_receiver,
            },
        );
    assert!(
        previous.is_none(),
        "firmware dispatch ownership pause already installed"
    );
    DispatchOwnershipPause {
        reached: reached_receiver,
        resume: Some(resume_sender),
    }
}

impl DispatchOwnershipPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        tokio::time::timeout(Duration::from_secs(5), &mut self.reached)
            .await
            .expect("timed out waiting for firmware dispatch ownership pause")
            .expect("firmware dispatch ownership pause was dropped before being reached");
    }

    pub(crate) fn resume(mut self) {
        let _ = self
            .resume
            .take()
            .expect("firmware dispatch ownership resume sender must be present")
            .send(());
    }
}

pub(crate) async fn wait(operation: &str, printer_id: &str) {
    let pause = pauses()
        .lock()
        .expect("firmware dispatch ownership pause mutex should not be poisoned")
        .remove(&pause_key(operation, printer_id));
    if let Some(pause) = pause {
        let _ = pause.reached.send(());
        let _ = pause.resume.await;
    }
}

fn pause_key(operation: &str, printer_id: &str) -> String {
    format!("{operation}:{printer_id}")
}

fn pauses() -> &'static Mutex<HashMap<String, PausePoint>> {
    static PAUSES: OnceLock<Mutex<HashMap<String, PausePoint>>> = OnceLock::new();
    PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}
