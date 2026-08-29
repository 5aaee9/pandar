use std::{
    collections::HashMap,
    sync::{Arc, RwLock as StdRwLock},
};

use pandar_core::BambuDeviceFeatures;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

#[derive(Debug, Default)]
struct DeviceFeatureEntry {
    value: StdRwLock<Option<BambuDeviceFeatures>>,
    transition: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceFeatureCache {
    entries: Arc<RwLock<HashMap<String, Arc<DeviceFeatureEntry>>>>,
}

pub(crate) struct DeviceFeatureLease {
    entry: Arc<DeviceFeatureEntry>,
    _guard: OwnedMutexGuard<()>,
}

impl DeviceFeatureLease {
    pub(crate) fn get(&self) -> Option<BambuDeviceFeatures> {
        *self.entry.value.read().unwrap()
    }

    pub(crate) fn set(&self, value: Option<BambuDeviceFeatures>) {
        *self.entry.value.write().unwrap() = value;
    }
}

impl DeviceFeatureCache {
    pub async fn get(&self, serial: &str) -> Option<BambuDeviceFeatures> {
        let entry = self.entries.read().await.get(serial).cloned()?;
        *entry.value.read().unwrap()
    }

    pub async fn update(&self, serial: &str, value: BambuDeviceFeatures) {
        self.transition_lease(serial).await.set(Some(value));
    }

    pub async fn invalidate(&self, serial: &str) {
        self.transition_lease(serial).await.set(None);
    }

    pub(crate) async fn transition_lease(&self, serial: &str) -> DeviceFeatureLease {
        let entry = self.entry(serial).await;
        #[cfg(test)]
        transition_pause::notify_waiting(serial);
        let guard = entry.transition.clone().lock_owned().await;
        DeviceFeatureLease {
            entry,
            _guard: guard,
        }
    }

    async fn entry(&self, serial: &str) -> Arc<DeviceFeatureEntry> {
        if let Some(entry) = self.entries.read().await.get(serial).cloned() {
            return entry;
        }
        self.entries
            .write()
            .await
            .entry(serial.to_owned())
            .or_default()
            .clone()
    }
}

#[cfg(test)]
pub(crate) mod transition_pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tokio::sync::oneshot;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    pub(crate) struct TransitionWaiting {
        reached: oneshot::Receiver<()>,
    }

    pub(crate) fn observe_waiting(serial: &str) -> TransitionWaiting {
        let (sender, receiver) = oneshot::channel();
        let previous = waiters().lock().unwrap().insert(serial.to_owned(), sender);
        assert!(
            previous.is_none(),
            "feature transition waiter already installed"
        );
        TransitionWaiting { reached: receiver }
    }

    impl TransitionWaiting {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for feature transition")
                .expect("feature transition waiter was dropped");
        }
    }

    pub(super) fn notify_waiting(serial: &str) {
        if let Some(waiter) = waiters().lock().unwrap().remove(serial) {
            let _ = waiter.send(());
        }
    }

    fn waiters() -> &'static Mutex<HashMap<String, oneshot::Sender<()>>> {
        static WAITERS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();
        WAITERS.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
