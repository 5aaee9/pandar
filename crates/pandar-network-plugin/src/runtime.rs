use std::sync::OnceLock;

pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("plugin runtime can be created")
    })
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    use super::*;

    #[test]
    fn runtime_is_reused_by_concurrent_callers() {
        assert!(std::ptr::eq(runtime(), runtime()));
        let ready = Arc::new(Barrier::new(3));
        let callers = (0..2)
            .map(|value| {
                let ready = Arc::clone(&ready);
                thread::spawn(move || {
                    ready.wait();
                    runtime().block_on(async move {
                        tokio::task::yield_now().await;
                        value
                    })
                })
            })
            .collect::<Vec<_>>();
        ready.wait();
        let values = callers
            .into_iter()
            .map(|caller| caller.join().expect("runtime caller"))
            .collect::<Vec<_>>();
        assert_eq!(values, vec![0, 1]);
    }
}
