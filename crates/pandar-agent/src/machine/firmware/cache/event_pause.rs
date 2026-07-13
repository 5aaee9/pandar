use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FirmwareEventKind {
    Modules,
    Status,
}

pub(crate) struct FirmwareEventPause {
    reached: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

struct PauseHook {
    reached: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

pub(crate) fn install(serial: &str, kind: FirmwareEventKind) -> FirmwareEventPause {
    let (reached_sender, reached) = oneshot::channel();
    let (release, release_receiver) = oneshot::channel();
    let previous = hooks().lock().unwrap().insert(
        (serial.to_owned(), kind),
        PauseHook {
            reached: reached_sender,
            release: release_receiver,
        },
    );
    assert!(previous.is_none(), "firmware event pause already installed");
    FirmwareEventPause { reached, release }
}

impl FirmwareEventPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        (&mut self.reached)
            .await
            .expect("firmware event pause was dropped before commit");
    }

    pub(crate) fn release(self) {
        let _ = self.release.send(());
    }
}

pub(crate) async fn after_commit(serial: &str, kind: FirmwareEventKind) {
    let hook = hooks().lock().unwrap().remove(&(serial.to_owned(), kind));
    if let Some(hook) = hook {
        let _ = hook.reached.send(());
        let _ = hook.release.await;
    }
}

fn hooks() -> &'static Mutex<HashMap<(String, FirmwareEventKind), PauseHook>> {
    static HOOKS: OnceLock<Mutex<HashMap<(String, FirmwareEventKind), PauseHook>>> =
        OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(HashMap::new()))
}
