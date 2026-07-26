use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use pandar_core::TenantId;

use super::CameraOpenError;

#[derive(Debug)]
pub(super) struct CameraCapacity {
    max_streams_per_tenant: usize,
    tenants: Mutex<HashMap<TenantId, usize>>,
}

pub(super) struct CameraCapacityPermit {
    capacity: Arc<CameraCapacity>,
    tenant_id: TenantId,
}

impl CameraCapacity {
    pub(super) fn new(max_streams_per_tenant: usize) -> Self {
        Self {
            max_streams_per_tenant,
            tenants: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn acquire(
        self: &Arc<Self>,
        tenant_id: TenantId,
    ) -> Result<CameraCapacityPermit, CameraOpenError> {
        {
            let mut tenants = self.tenants.lock().expect("camera capacity tenants");
            let count = tenants.entry(tenant_id).or_default();
            if *count >= self.max_streams_per_tenant {
                return Err(CameraOpenError::Capacity);
            }
            *count += 1;
        }
        Ok(CameraCapacityPermit {
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
        let capacity = Arc::new(CameraCapacity::new(2));
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
    fn capacity_has_no_global_limit() {
        let capacity = Arc::new(CameraCapacity::new(8));
        let permits = (0..64)
            .map(|_| capacity.acquire(TenantId::new()).unwrap())
            .collect::<Vec<_>>();

        drop(permits);
        assert!(capacity.acquire(TenantId::new()).is_ok());
    }
}
