use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::oneshot;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
enum Phase {
    AfterSerialization,
    DuringFlush,
}

struct PausePoint {
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

#[derive(Default)]
struct PauseSlots {
    after_serialization: Option<PausePoint>,
    during_flush: Option<PausePoint>,
}

pub(crate) struct SendPause {
    phase: Phase,
    reached: oneshot::Receiver<()>,
    resume: Option<oneshot::Sender<()>>,
}

pub(crate) fn install_after_serialization() -> SendPause {
    install(Phase::AfterSerialization)
}

pub(crate) fn install_during_flush() -> SendPause {
    install(Phase::DuringFlush)
}

fn install(phase: Phase) -> SendPause {
    let (reached_sender, reached_receiver) = oneshot::channel();
    let (resume_sender, resume_receiver) = oneshot::channel();
    let mut slots = slots()
        .lock()
        .expect("printer event send pause mutex should not be poisoned");
    let slot = slot_mut(&mut slots, phase);
    assert!(slot.is_none(), "printer event send pause already installed");
    *slot = Some(PausePoint {
        reached: reached_sender,
        resume: resume_receiver,
    });
    SendPause {
        phase,
        reached: reached_receiver,
        resume: Some(resume_sender),
    }
}

impl SendPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
            .await
            .expect("timed out waiting for printer event send pause")
            .expect("printer event send pause was dropped before being reached");
    }

    pub(crate) fn resume(mut self) {
        let _ = self
            .resume
            .take()
            .expect("printer event send pause resume sender must be present")
            .send(());
    }
}

impl Drop for SendPause {
    fn drop(&mut self) {
        let mut slots = slots()
            .lock()
            .expect("printer event send pause mutex should not be poisoned");
        slot_mut(&mut slots, self.phase).take();
    }
}

pub(crate) async fn wait_after_serialization() {
    wait(Phase::AfterSerialization).await;
}

pub(crate) async fn wait_during_flush() {
    wait(Phase::DuringFlush).await;
}

async fn wait(phase: Phase) {
    let pause = {
        let mut slots = slots()
            .lock()
            .expect("printer event send pause mutex should not be poisoned");
        slot_mut(&mut slots, phase).take()
    };
    if let Some(pause) = pause {
        let _ = pause.reached.send(());
        let _ = pause.resume.await;
    }
}

fn slot_mut(slots: &mut PauseSlots, phase: Phase) -> &mut Option<PausePoint> {
    match phase {
        Phase::AfterSerialization => &mut slots.after_serialization,
        Phase::DuringFlush => &mut slots.during_flush,
    }
}

fn slots() -> &'static Mutex<PauseSlots> {
    static SLOTS: OnceLock<Mutex<PauseSlots>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(PauseSlots::default()))
}
