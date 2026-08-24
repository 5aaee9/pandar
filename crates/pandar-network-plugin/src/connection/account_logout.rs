use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{ConnectionSession, no_auth_rotation::NoAuthRotationOutcome};

mod state;

use state::{phase, state_generation, state_word, unregister_follower};

#[cfg(test)]
#[path = "account_logout/tests.rs"]
mod tests;

const PHASE_MASK: u64 = 0b111;
const PHASE_IDLE: u64 = 0;
const PHASE_PASSIVE_ACTIVE: u64 = 1;
const PHASE_REQUESTED_ACTIVE: u64 = 2;
const PHASE_PASSIVE_FINALIZING: u64 = 3;
const PHASE_REQUESTED_FINALIZING: u64 = 4;
const PHASE_PASSIVE_COMMITTED: u64 = 5;
const PHASE_REQUESTED_COMMITTED: u64 = 6;

thread_local! {
    static ACCOUNT_LOGOUT_OWNER: Cell<*const ConnectionSession> = const { Cell::new(std::ptr::null()) };
}

pub(super) struct AccountLogoutCoordinator {
    state: AtomicU64,
    completions: Mutex<BTreeMap<u64, Completion>>,
    changed: Condvar,
    #[cfg(test)]
    committed_waiters: std::sync::atomic::AtomicUsize,
}

struct Completion {
    followers: usize,
    outcome: Option<NoAuthRotationOutcome>,
}

pub(crate) enum AccountLogoutBegin<'a> {
    Owner(AccountLogoutOwner<'a>),
    Follower(AccountLogoutFollower<'a>),
    Immediate,
}

pub(crate) struct AccountLogoutOwner<'a> {
    session: &'a ConnectionSession,
    generation: u64,
    previous_owner: *const ConnectionSession,
    completed: bool,
}

pub(crate) struct AccountLogoutFollower<'a> {
    coordinator: &'a AccountLogoutCoordinator,
    generation: u64,
}

impl AccountLogoutCoordinator {
    pub(super) fn new() -> Self {
        Self {
            state: AtomicU64::new(state_word(0, PHASE_IDLE)),
            completions: Mutex::new(BTreeMap::new()),
            changed: Condvar::new(),
            #[cfg(test)]
            committed_waiters: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn begin<'a>(
        &'a self,
        session: &'a ConnectionSession,
        request: bool,
    ) -> AccountLogoutBegin<'a> {
        loop {
            let observed = self.state.load(Ordering::Acquire);
            let generation = state_generation(observed);
            match phase(observed) {
                PHASE_IDLE => {
                    let next_generation = generation.wrapping_add(1);
                    let next_phase = if request {
                        PHASE_REQUESTED_ACTIVE
                    } else {
                        PHASE_PASSIVE_ACTIVE
                    };
                    if self
                        .state
                        .compare_exchange(
                            observed,
                            state_word(next_generation, next_phase),
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        let previous_owner =
                            ACCOUNT_LOGOUT_OWNER.with(|owner| owner.replace(session));
                        return AccountLogoutBegin::Owner(AccountLogoutOwner {
                            session,
                            generation: next_generation,
                            previous_owner,
                            completed: false,
                        });
                    }
                }
                PHASE_PASSIVE_ACTIVE if request => {
                    let mut completions = self.completions.lock().expect("account logout outcome");
                    if self.state.load(Ordering::Acquire) != observed {
                        continue;
                    }
                    let completion = completions.entry(generation).or_insert(Completion {
                        followers: 0,
                        outcome: None,
                    });
                    completion.followers += 1;
                    if self
                        .state
                        .compare_exchange(
                            observed,
                            state_word(generation, PHASE_REQUESTED_ACTIVE),
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        self.changed.notify_all();
                        return AccountLogoutBegin::Follower(AccountLogoutFollower {
                            coordinator: self,
                            generation,
                        });
                    }
                    unregister_follower(&mut completions, generation);
                }
                PHASE_REQUESTED_ACTIVE | PHASE_REQUESTED_FINALIZING | PHASE_REQUESTED_COMMITTED
                    if request =>
                {
                    let mut completions = self.completions.lock().expect("account logout outcome");
                    let current = self.state.load(Ordering::Acquire);
                    if state_generation(current) != generation
                        || !matches!(
                            phase(current),
                            PHASE_REQUESTED_ACTIVE
                                | PHASE_REQUESTED_FINALIZING
                                | PHASE_REQUESTED_COMMITTED
                        )
                    {
                        continue;
                    }
                    completions
                        .entry(generation)
                        .and_modify(|completion| completion.followers += 1)
                        .or_insert(Completion {
                            followers: 1,
                            outcome: None,
                        });
                    self.changed.notify_all();
                    return AccountLogoutBegin::Follower(AccountLogoutFollower {
                        coordinator: self,
                        generation,
                    });
                }
                PHASE_PASSIVE_FINALIZING if request => {
                    let mut completions = self.completions.lock().expect("account logout outcome");
                    if self.state.load(Ordering::Acquire) != observed {
                        continue;
                    }
                    let completion = completions.entry(generation).or_insert(Completion {
                        followers: 0,
                        outcome: None,
                    });
                    completion.followers += 1;
                    if self
                        .state
                        .compare_exchange(
                            observed,
                            state_word(generation, PHASE_REQUESTED_FINALIZING),
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        self.changed.notify_all();
                        return AccountLogoutBegin::Follower(AccountLogoutFollower {
                            coordinator: self,
                            generation,
                        });
                    }
                    unregister_follower(&mut completions, generation);
                }
                PHASE_PASSIVE_COMMITTED if request => {
                    let completions = self.completions.lock().expect("account logout outcome");
                    if self.state.load(Ordering::Acquire) == observed {
                        #[cfg(test)]
                        self.committed_waiters.fetch_add(1, Ordering::Release);
                        #[cfg(test)]
                        self.changed.notify_all();
                        drop(
                            self.changed
                                .wait_while(completions, |_| {
                                    self.state.load(Ordering::Acquire) == observed
                                })
                                .expect("account logout outcome"),
                        );
                        #[cfg(test)]
                        self.committed_waiters.fetch_sub(1, Ordering::Release);
                    }
                }
                _ => return AccountLogoutBegin::Immediate,
            }
        }
    }

    fn upgrade_reentrant(&self) {
        loop {
            let observed = self.state.load(Ordering::Acquire);
            let next_phase = match phase(observed) {
                PHASE_PASSIVE_ACTIVE => PHASE_REQUESTED_ACTIVE,
                PHASE_PASSIVE_FINALIZING => PHASE_REQUESTED_FINALIZING,
                _ => return,
            };
            if self
                .state
                .compare_exchange(
                    observed,
                    state_word(state_generation(observed), next_phase),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return;
            }
        }
    }

    fn begin_finalization(&self, generation: u64) {
        loop {
            let observed = self.state.load(Ordering::Acquire);
            if state_generation(observed) != generation {
                return;
            }
            let next_phase = match phase(observed) {
                PHASE_PASSIVE_ACTIVE => PHASE_PASSIVE_FINALIZING,
                PHASE_REQUESTED_ACTIVE => PHASE_REQUESTED_FINALIZING,
                PHASE_PASSIVE_FINALIZING
                | PHASE_REQUESTED_FINALIZING
                | PHASE_PASSIVE_COMMITTED
                | PHASE_REQUESTED_COMMITTED => return,
                _ => return,
            };
            if self
                .state
                .compare_exchange(
                    observed,
                    state_word(generation, next_phase),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return;
            }
        }
    }

    fn seal_finalization(&self, generation: u64) -> bool {
        loop {
            let observed = self.state.load(Ordering::Acquire);
            if state_generation(observed) != generation {
                return false;
            }
            let (next_phase, requested) = match phase(observed) {
                PHASE_PASSIVE_FINALIZING => (PHASE_PASSIVE_COMMITTED, false),
                PHASE_REQUESTED_FINALIZING => (PHASE_REQUESTED_COMMITTED, true),
                PHASE_PASSIVE_COMMITTED => return false,
                PHASE_REQUESTED_COMMITTED => return true,
                _ => return false,
            };
            if self
                .state
                .compare_exchange(
                    observed,
                    state_word(generation, next_phase),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return requested;
            }
        }
    }

    fn complete(&self, generation: u64, requested: bool, outcome: NoAuthRotationOutcome) {
        let mut completions = self.completions.lock().expect("account logout outcome");
        if requested && let Some(completion) = completions.get_mut(&generation) {
            completion.outcome = Some(outcome);
        }
        self.state
            .store(state_word(generation, PHASE_IDLE), Ordering::Release);
        self.changed.notify_all();
    }

    fn abort(&self, generation: u64) {
        self.begin_finalization(generation);
        let requested = self.seal_finalization(generation);
        self.complete(
            generation,
            requested,
            NoAuthRotationOutcome {
                status: 1,
                http_code: 0,
                body: crate::stable_error_body("account_state_unavailable"),
            },
        );
    }

    fn in_flight(&self) -> bool {
        phase(self.state.load(Ordering::Acquire)) != PHASE_IDLE
    }
}

impl ConnectionSession {
    pub(crate) fn begin_account_logout(&self, request: bool) -> AccountLogoutBegin<'_> {
        let reentrant = ACCOUNT_LOGOUT_OWNER.with(|owner| std::ptr::eq(owner.get(), self));
        if reentrant {
            if request {
                self.account_logout.upgrade_reentrant();
            }
            return AccountLogoutBegin::Immediate;
        }
        self.account_logout.begin(self, request)
    }

    pub(crate) fn account_logout_in_flight(&self) -> bool {
        self.account_logout.in_flight()
    }
}

impl AccountLogoutOwner<'_> {
    pub(crate) fn begin_finalization(&mut self) {
        self.session
            .account_logout
            .begin_finalization(self.generation);
    }

    pub(crate) fn seal_finalization(&mut self) -> bool {
        self.session
            .account_logout
            .seal_finalization(self.generation)
    }

    pub(crate) fn complete(mut self, requested: bool, outcome: NoAuthRotationOutcome) {
        self.session
            .account_logout
            .complete(self.generation, requested, outcome);
        self.session.notify_dispatcher();
        self.completed = true;
    }
}

impl Drop for AccountLogoutOwner<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.session.account_logout.abort(self.generation);
        }
        ACCOUNT_LOGOUT_OWNER.with(|owner| owner.set(self.previous_owner));
    }
}
