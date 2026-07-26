use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const MAX_LOCAL_CONNECTIONS: usize = 32;

pub(super) struct ConnectionPermit(Arc<AtomicUsize>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

pub(super) fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

pub(super) fn try_acquire(counter: &Arc<AtomicUsize>) -> Option<ConnectionPermit> {
    counter
        .fetch_update(Ordering::Acquire, Ordering::Relaxed, |active| {
            (active < MAX_LOCAL_CONNECTIONS).then_some(active + 1)
        })
        .ok()
        .map(|_| ConnectionPermit(Arc::clone(counter)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_connection_permits_are_bounded_and_released() {
        let counter = counter();
        let mut permits = (0..MAX_LOCAL_CONNECTIONS)
            .map(|_| try_acquire(&counter).unwrap())
            .collect::<Vec<_>>();
        assert!(try_acquire(&counter).is_none());

        permits.pop();
        assert!(try_acquire(&counter).is_some());
    }
}
