use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use pandar_core::TenantId;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::CameraOpenError;

const MAX_CAMERA_STREAMS: usize = 32;
const MAX_CAMERA_STREAMS_PER_TENANT: usize = 2;

#[derive(Debug)]
pub(super) struct CameraCapacity {
    global: Arc<Semaphore>,
    tenants: Mutex<HashMap<TenantId, usize>>,
}

pub(super) struct CameraCapacityPermit {
    _global: OwnedSemaphorePermit,
    capacity: Arc<CameraCapacity>,
    tenant_id: TenantId,
}

impl CameraCapacity {
    pub(super) fn new() -> Self {
        Self {
            global: Arc::new(Semaphore::new(MAX_CAMERA_STREAMS)),
            tenants: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn acquire(
        self: &Arc<Self>,
        tenant_id: TenantId,
    ) -> Result<CameraCapacityPermit, CameraOpenError> {
        let global = Arc::clone(&self.global)
            .try_acquire_owned()
            .map_err(|_| CameraOpenError::Capacity)?;
        {
            let mut tenants = self.tenants.lock().expect("camera capacity tenants");
            let count = tenants.entry(tenant_id).or_default();
            if *count >= MAX_CAMERA_STREAMS_PER_TENANT {
                return Err(CameraOpenError::Capacity);
            }
            *count += 1;
        }
        Ok(CameraCapacityPermit {
            _global: global,
            capacity: Arc::clone(self),
            tenant_id,
        })
    }
}

impl Drop for CameraCapacityPermit {
    fn drop(&mut self) {
        let mut tenants = self
            .capacity
            .tenants
            .lock()
            .expect("camera capacity tenants");
        let count = tenants
            .get_mut(&self.tenant_id)
            .expect("camera tenant capacity permit");
        *count -= 1;
        if *count == 0 {
            tenants.remove(&self.tenant_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_is_bounded_per_tenant_and_released() {
        let capacity = Arc::new(CameraCapacity::new());
        let tenant_id = TenantId::new();
        let first = capacity.acquire(tenant_id).unwrap();
        let _second = capacity.acquire(tenant_id).unwrap();
        assert!(matches!(
            capacity.acquire(tenant_id),
            Err(CameraOpenError::Capacity)
        ));
        assert!(capacity.acquire(TenantId::new()).is_ok());

        drop(first);
        assert!(capacity.acquire(tenant_id).is_ok());
    }

    #[test]
    fn capacity_is_bounded_globally() {
        let capacity = Arc::new(CameraCapacity::new());
        let permits = (0..MAX_CAMERA_STREAMS)
            .map(|_| capacity.acquire(TenantId::new()).unwrap())
            .collect::<Vec<_>>();
        assert!(matches!(
            capacity.acquire(TenantId::new()),
            Err(CameraOpenError::Capacity)
        ));

        drop(permits);
        assert!(capacity.acquire(TenantId::new()).is_ok());
    }
}
