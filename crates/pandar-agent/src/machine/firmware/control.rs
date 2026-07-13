use std::sync::atomic::Ordering;
use std::{fmt, sync::Arc, time::Duration};

use anyhow::{anyhow, bail};
use tokio::sync::OwnedMutexGuard;

use super::{
    FirmwareExecuteRequest, FirmwareObservationCache, FirmwarePrepareRequest,
    FirmwarePreparedObservation, FirmwareReservationState,
    cache::{FirmwareControlReservation, FirmwareEntry},
};
use crate::machine::BambuPrinterEndpoint;

const PREPARATION_LIFETIME: Duration = Duration::from_secs(1);

pub struct FirmwareExecutionLease {
    entry: Arc<FirmwareEntry>,
    state: FirmwareReservationState,
}

pub struct FirmwarePublishTransition {
    endpoint: BambuPrinterEndpoint,
    guard: Option<OwnedMutexGuard<()>>,
    #[cfg(test)]
    release_observer: Option<Box<dyn FnOnce() + Send>>,
}

impl FirmwareObservationCache {
    pub async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        let entry = self.entry(&request.serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let ended_epoch = self.ended_session_epoch.load(Ordering::SeqCst);
        if ended_epoch != 0 && request.session_epoch <= ended_epoch {
            bail!("firmware command belongs to an ended reverse session");
        }
        let expires_at = tokio::time::Instant::now() + PREPARATION_LIFETIME;
        let reservation = FirmwareReservationState {
            command_id: request.command_id.clone(),
            session_epoch: request.session_epoch,
            generation: request.expected_generation,
        };
        {
            let mut state = entry.state.write().unwrap();
            if state.endpoint.is_none() || state.generation != request.expected_generation {
                bail!(
                    "stale firmware generation {} for printer {}",
                    request.expected_generation,
                    request.serial
                );
            }
            expire_prepared(&mut state);
            if state.reservation.is_some() {
                bail!("printer {} firmware control is busy", request.serial);
            }
            state.reservation = Some(FirmwareControlReservation {
                state: reservation.clone(),
                expires_at,
                in_flight: false,
            });
        }
        Ok(FirmwarePreparedObservation {
            command_id: request.command_id,
            serial: request.serial,
            generation: request.expected_generation,
        })
    }

    pub async fn claim_firmware_execute(
        &self,
        request: &FirmwareExecuteRequest,
    ) -> anyhow::Result<FirmwareExecutionLease> {
        let entry = self.entry(&request.serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let ended_epoch = self.ended_session_epoch.load(Ordering::SeqCst);
        if ended_epoch != 0 && request.session_epoch <= ended_epoch {
            bail!("firmware command belongs to an ended reverse session");
        }
        let expected = FirmwareReservationState {
            command_id: request.command_id.clone(),
            session_epoch: request.session_epoch,
            generation: request.expected_generation,
        };
        {
            let mut state = entry.state.write().unwrap();
            if state.endpoint.is_none() || state.generation != request.expected_generation {
                bail!("stale firmware generation for printer {}", request.serial);
            }
            expire_prepared(&mut state);
            let reservation = state.reservation.as_mut().ok_or_else(|| {
                anyhow!(
                    "no prepared firmware control for printer {}",
                    request.serial
                )
            })?;
            if reservation.state.command_id != expected.command_id {
                bail!("firmware command does not match prepared command");
            }
            if reservation.state.session_epoch != expected.session_epoch {
                bail!("firmware command belongs to a different reverse session");
            }
            if reservation.state.generation != expected.generation {
                bail!("firmware command generation does not match preparation");
            }
            if reservation.in_flight {
                bail!("printer {} firmware control is busy", request.serial);
            }
            reservation.in_flight = true;
        }
        Ok(FirmwareExecutionLease {
            entry,
            state: expected,
        })
    }

    pub async fn cancel_firmware_session(&self, session_epoch: u64) {
        self.ended_session_epoch
            .fetch_max(session_epoch, Ordering::SeqCst);
        let entries = self
            .entries
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            let _guard = entry.transition.clone().lock_owned().await;
            let mut state = entry.state.write().unwrap();
            if state
                .reservation
                .as_ref()
                .is_some_and(|reservation| reservation.state.session_epoch == session_epoch)
            {
                state.reservation = None;
            }
        }
    }
}

impl FirmwareExecutionLease {
    pub async fn publish_transition(&self) -> anyhow::Result<FirmwarePublishTransition> {
        let guard = self.entry.transition.clone().lock_owned().await;
        let endpoint = {
            let state = self.entry.state.read().unwrap();
            let matches = state.reservation.as_ref().is_some_and(|reservation| {
                reservation.in_flight && reservation.state == self.state
            });
            if !matches || state.generation != self.state.generation {
                bail!("firmware execution reservation is no longer current");
            }
            state
                .endpoint
                .clone()
                .expect("current firmware generation has an endpoint")
        };
        Ok(FirmwarePublishTransition {
            endpoint,
            guard: Some(guard),
            #[cfg(test)]
            release_observer: None,
        })
    }
}

impl FirmwarePublishTransition {
    pub fn endpoint(&self) -> &BambuPrinterEndpoint {
        &self.endpoint
    }

    #[cfg(test)]
    pub(crate) fn observe_release_for_test(&mut self, observer: impl FnOnce() + Send + 'static) {
        self.release_observer = Some(Box::new(observer));
    }
}

impl Drop for FirmwarePublishTransition {
    fn drop(&mut self) {
        drop(self.guard.take());
        #[cfg(test)]
        if let Some(observer) = self.release_observer.take() {
            observer();
        }
    }
}

impl fmt::Debug for FirmwareExecutionLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FirmwareExecutionLease")
            .field("state", &self.state)
            .finish()
    }
}

impl Drop for FirmwareExecutionLease {
    fn drop(&mut self) {
        let mut state = self.entry.state.write().unwrap();
        if state
            .reservation
            .as_ref()
            .is_some_and(|reservation| reservation.in_flight && reservation.state == self.state)
        {
            state.reservation = None;
        }
    }
}

pub(super) fn expire_prepared(state: &mut super::cache::FirmwareEntryState) {
    if state.reservation.as_ref().is_some_and(|reservation| {
        !reservation.in_flight && tokio::time::Instant::now() >= reservation.expires_at
    }) {
        state.reservation = None;
    }
}
