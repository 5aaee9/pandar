use std::sync::{Mutex, OnceLock};

use tokio::sync::oneshot;

struct Pause {
    name: String,
    counted: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

pub(crate) struct PauseHandle {
    counted: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

fn slot() -> &'static Mutex<Option<Pause>> {
    static SLOT: OnceLock<Mutex<Option<Pause>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub(crate) fn install(name: impl Into<String>) -> PauseHandle {
    let (counted_tx, counted_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let previous = slot().lock().unwrap().replace(Pause {
        name: name.into(),
        counted: counted_tx,
        release: release_rx,
    });
    assert!(
        previous.is_none(),
        "no-auth session pause already installed"
    );
    PauseHandle {
        counted: counted_rx,
        release: release_tx,
    }
}

impl PauseHandle {
    pub(crate) async fn wait_until_counted(&mut self) {
        (&mut self.counted).await.unwrap();
    }

    pub(crate) fn release(self) {
        self.release.send(()).unwrap();
    }
}

pub(super) async fn wait(name: &str) {
    let pause = {
        let mut slot = slot().lock().unwrap();
        match slot.as_ref() {
            Some(pause) if pause.name == name => slot.take(),
            _ => None,
        }
    };
    if let Some(pause) = pause {
        let _ = pause.counted.send(());
        let _ = pause.release.await;
    }
}
