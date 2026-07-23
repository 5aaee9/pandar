use std::collections::BTreeMap;

#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::{AccountLogoutCoordinator, ConnectionSession};
use super::{AccountLogoutFollower, Completion, NoAuthRotationOutcome, PHASE_MASK};

impl AccountLogoutFollower<'_> {
    pub(crate) fn wait(self) -> NoAuthRotationOutcome {
        let mut completions = self
            .coordinator
            .completions
            .lock()
            .expect("account logout outcome");
        loop {
            let completion = completions
                .get_mut(&self.generation)
                .expect("registered account logout outcome");
            if let Some(outcome) = completion.outcome.clone() {
                completion.followers -= 1;
                if completion.followers == 0 {
                    completions.remove(&self.generation);
                }
                return outcome;
            }
            completions = self
                .coordinator
                .changed
                .wait(completions)
                .expect("account logout outcome");
        }
    }
}

pub(super) fn unregister_follower(completions: &mut BTreeMap<u64, Completion>, generation: u64) {
    let completion = completions
        .get_mut(&generation)
        .expect("registered account logout outcome");
    completion.followers -= 1;
    if completion.followers == 0 {
        completions.remove(&generation);
    }
}

pub(super) fn state_word(generation: u64, phase: u64) -> u64 {
    (generation << 3) | phase
}

pub(super) fn state_generation(state: u64) -> u64 {
    state >> 3
}

pub(super) fn phase(state: u64) -> u64 {
    state & PHASE_MASK
}

#[cfg(test)]
impl AccountLogoutCoordinator {
    fn wait_for_follower(&self) {
        let completions = self.completions.lock().expect("account logout outcome");
        drop(
            self.changed
                .wait_while(completions, |completions| {
                    completions
                        .values()
                        .all(|completion| completion.followers == 0)
                })
                .expect("account logout outcome"),
        );
    }

    fn wait_for_committed_waiter(&self) {
        let completions = self.completions.lock().expect("account logout outcome");
        drop(
            self.changed
                .wait_while(completions, |_| {
                    self.committed_waiters.load(Ordering::Acquire) == 0
                })
                .expect("account logout outcome"),
        );
    }
}

#[cfg(test)]
impl ConnectionSession {
    pub(crate) fn wait_for_account_logout_follower(&self) {
        self.account_logout.wait_for_follower();
    }

    pub(super) fn wait_for_account_logout_committed_waiter(&self) {
        self.account_logout.wait_for_committed_waiter();
    }
}
