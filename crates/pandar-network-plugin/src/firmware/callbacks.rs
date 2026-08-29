use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

#[cfg(test)]
pub(crate) mod test_hook;
#[cfg(test)]
mod tests;

const CALLBACK_NOT_BEFORE: Duration = Duration::from_millis(1_100);
const CALLBACK_DEADLINE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmwareTunnel {
    Cloud,
    Local,
}

pub struct FirmwareCallback {
    pub dev_id: String,
    pub tunnel: FirmwareTunnel,
    pub message: String,
}

pub struct ReadyFirmwareCallback {
    pub token: u64,
    pub generation: u64,
    pub origin_tick: u64,
    pub local_generation: u64,
    pub cache_generation: u64,
    pub dev_id: String,
    pub tunnel: FirmwareTunnel,
    pub message: String,
}

pub struct FirmwareCallbackQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

struct QueueState {
    next_token: u64,
    pending: VecDeque<PendingFirmwareCallback>,
    stopped: bool,
}

struct PendingFirmwareCallback {
    token: u64,
    generation: u64,
    callback: FirmwareCallback,
    handoff: Option<CallbackHandoff>,
}

struct CallbackHandoff {
    origin_tick: u64,
    local_generation: u64,
    cache_generation: u64,
    not_before: Instant,
    deadline: Instant,
}

impl FirmwareCallbackQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                next_token: 1,
                pending: VecDeque::new(),
                stopped: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub fn push(&self, generation: u64, callback: FirmwareCallback) -> Option<u64> {
        #[cfg(test)]
        test_hook::pause_before_push();
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        if state.stopped {
            return None;
        }
        let token = state.next_token;
        state.next_token += 1;
        state.pending.push_back(PendingFirmwareCallback {
            token,
            generation,
            callback,
            handoff: None,
        });
        Some(token)
    }

    pub fn return_handoff_at(
        &self,
        token: u64,
        origin_tick: u64,
        local_generation: u64,
        cache_generation: u64,
        handoff: Instant,
    ) -> bool {
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        if state.stopped {
            return false;
        }
        let Some(pending) = state
            .pending
            .iter_mut()
            .find(|pending| pending.token == token && pending.handoff.is_none())
        else {
            return false;
        };
        pending.handoff = Some(CallbackHandoff {
            origin_tick,
            local_generation,
            cache_generation,
            not_before: handoff + CALLBACK_NOT_BEFORE,
            deadline: handoff + CALLBACK_DEADLINE,
        });
        self.ready.notify_one();
        true
    }

    pub fn take_ready_at(&self, now: Instant) -> Option<ReadyFirmwareCallback> {
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        if state.stopped {
            return None;
        }
        take_ready(&mut state, now)
    }

    pub fn cancel_generation(&self, generation: u64) {
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        state
            .pending
            .retain(|pending| pending.generation != generation);
        self.ready.notify_all();
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        state.stopped = true;
        state.pending.clear();
        self.ready.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        self.state
            .lock()
            .expect("firmware callback queue poisoned")
            .stopped
    }

    pub fn wait_ready(&self, timeout: Duration) -> Option<ReadyFirmwareCallback> {
        let timeout_at = Instant::now() + timeout;
        let mut state = self.state.lock().expect("firmware callback queue poisoned");
        loop {
            if state.stopped {
                return None;
            }
            let now = Instant::now();
            if let Some(callback) = take_ready(&mut state, now) {
                return Some(callback);
            }
            let remaining = timeout_at.checked_duration_since(now)?;
            let wait_for = state
                .pending
                .iter()
                .filter_map(|pending| pending.handoff.as_ref())
                .filter_map(|handoff| handoff.not_before.checked_duration_since(now))
                .min()
                .map_or(remaining, |until_ready| remaining.min(until_ready));
            if wait_for.is_zero() {
                return None;
            }
            #[cfg(test)]
            test_hook::callback_wait_entered();
            let (next_state, _) = self
                .ready
                .wait_timeout(state, wait_for)
                .expect("firmware callback queue poisoned");
            state = next_state;
        }
    }
}

impl Default for FirmwareCallbackQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn take_ready(state: &mut QueueState, now: Instant) -> Option<ReadyFirmwareCallback> {
    state.pending.retain(|pending| {
        pending
            .handoff
            .as_ref()
            .is_none_or(|handoff| now < handoff.deadline)
    });
    let index = state.pending.iter().position(|pending| {
        pending
            .handoff
            .as_ref()
            .is_some_and(|handoff| now >= handoff.not_before)
    })?;
    let pending = state.pending.remove(index)?;
    let handoff = pending.handoff?;
    Some(ReadyFirmwareCallback {
        token: pending.token,
        generation: pending.generation,
        origin_tick: handoff.origin_tick,
        local_generation: handoff.local_generation,
        cache_generation: handoff.cache_generation,
        dev_id: pending.callback.dev_id,
        tunnel: pending.callback.tunnel,
        message: pending.callback.message,
    })
}
