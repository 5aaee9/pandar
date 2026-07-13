use std::{
    sync::{Condvar, LazyLock, Mutex},
    time::{Duration, Instant},
};

static PUSH_PAUSE: LazyLock<(Mutex<PauseState>, Condvar)> =
    LazyLock::new(|| (Mutex::new(PauseState::default()), Condvar::new()));
static CALLBACK_WAIT: LazyLock<(Mutex<WaitState>, Condvar)> =
    LazyLock::new(|| (Mutex::new(WaitState::default()), Condvar::new()));

#[derive(Default)]
struct PauseState {
    armed: bool,
    reached: bool,
    released: bool,
}

#[derive(Default)]
struct WaitState {
    armed: bool,
    entered: bool,
}

pub(crate) fn arm() {
    let (state, _) = &*PUSH_PAUSE;
    *state.lock().expect("callback push pause poisoned") = PauseState {
        armed: true,
        reached: false,
        released: false,
    };
}

pub(crate) fn wait_until_reached() {
    let (state, ready) = &*PUSH_PAUSE;
    let mut state = state.lock().expect("callback push pause poisoned");
    while !state.reached {
        state = ready.wait(state).expect("callback push pause poisoned");
    }
}

pub(crate) fn release() {
    let (state, ready) = &*PUSH_PAUSE;
    let mut state = state.lock().expect("callback push pause poisoned");
    state.released = true;
    ready.notify_all();
}

pub(super) fn pause_before_push() {
    let (state, ready) = &*PUSH_PAUSE;
    let mut state = state.lock().expect("callback push pause poisoned");
    if !state.armed {
        return;
    }
    state.reached = true;
    ready.notify_all();
    while !state.released {
        state = ready.wait(state).expect("callback push pause poisoned");
    }
    state.armed = false;
}

pub(crate) fn arm_callback_wait() {
    let (state, _) = &*CALLBACK_WAIT;
    *state.lock().expect("callback wait hook poisoned") = WaitState {
        armed: true,
        entered: false,
    };
}

pub(crate) fn wait_until_callback_wait_entered(timeout: Duration) -> bool {
    let (state, ready) = &*CALLBACK_WAIT;
    let mut state = state.lock().expect("callback wait hook poisoned");
    let deadline = Instant::now() + timeout;
    while !state.entered {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return false;
        };
        let (next_state, wait) = ready
            .wait_timeout(state, remaining)
            .expect("callback wait hook poisoned");
        state = next_state;
        if wait.timed_out() && !state.entered {
            return false;
        }
    }
    true
}

pub(super) fn callback_wait_entered() {
    let (state, ready) = &*CALLBACK_WAIT;
    let mut state = state.lock().expect("callback wait hook poisoned");
    if !state.armed {
        return;
    }
    state.armed = false;
    state.entered = true;
    ready.notify_all();
}
