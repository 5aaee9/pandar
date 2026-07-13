use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use super::FirmwarePumpDropPause;

pub(super) struct PumpDropPauseFuture<F> {
    inner: Pin<Box<F>>,
    pause: Option<FirmwarePumpDropPause>,
    completed: bool,
}

impl<F> PumpDropPauseFuture<F> {
    pub(super) fn new(inner: F, pause: Option<FirmwarePumpDropPause>) -> Self {
        Self {
            inner: Box::pin(inner),
            pause,
            completed: false,
        }
    }
}

impl<F: Future> Future for PumpDropPauseFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let result = self.inner.as_mut().poll(context);
        if result.is_ready() {
            self.completed = true;
        }
        result
    }
}

impl<F> Drop for PumpDropPauseFuture<F> {
    fn drop(&mut self) {
        if !self.completed
            && let Some(pause) = self.pause.take()
        {
            pause.block_until_released();
        }
    }
}
