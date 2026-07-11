use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;

use super::*;
use crate::cluster::{ControlMessageStream, ControlPlane, ControlPlaneBackend, HubControlMessage};

#[derive(Debug, Default)]
struct FailOnceControlPlaneBackend {
    subscribe_calls: AtomicUsize,
}

#[async_trait]
impl ControlPlaneBackend for FailOnceControlPlaneBackend {
    async fn publish(&self, _message: HubControlMessage) -> anyhow::Result<()> {
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        let attempt = self.subscribe_calls.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            return Err(anyhow::anyhow!("transient NATS subscribe failure")
                .context("scripted startup subscription failure"));
        }
        Ok(Box::pin(futures_util::stream::pending()))
    }
}

#[tokio::test]
async fn startup_waits_for_control_plane_subscription_retry() {
    let backend = Arc::new(FailOnceControlPlaneBackend::default());
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::for_tests(backend.clone()));

    let control_plane = tokio::time::timeout(Duration::from_secs(2), start_control_plane(state))
        .await
        .expect("startup should reach the successful retry")
        .expect("a transient subscription failure must not stop startup");

    assert_eq!(backend.subscribe_calls.load(Ordering::SeqCst), 2);
    control_plane.abort();
}
