use std::{
    collections::HashMap,
    sync::{Arc, RwLock as StdRwLock, atomic::AtomicU64},
};

use anyhow::{Context, anyhow};
use pandar_core::{PrinterFirmwareModule, PrinterFirmwareStatus};
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock, mpsc};

use crate::{AgentConfig, machine::BambuPrinterEndpoint};
use pandar_protocol::agent::v1::AgentEvent;

use super::{
    control::expire_prepared,
    types::{
        FirmwareCacheSnapshot, FirmwareModulesObservation, FirmwareReservationState,
        FirmwareStatusObservation, firmware_invalidated_event, firmware_modules_event,
        firmware_status_event,
    },
};

#[derive(Debug, Default)]
pub(super) struct FirmwareEntryState {
    pub(super) endpoint: Option<BambuPrinterEndpoint>,
    pub(super) generation: u64,
    next_generation: u64,
    module_revision: u64,
    status_revision: u64,
    modules: Option<Vec<PrinterFirmwareModule>>,
    status: Option<PrinterFirmwareStatus>,
    pub(super) reservation: Option<FirmwareControlReservation>,
}

#[derive(Debug)]
pub(super) struct FirmwareControlReservation {
    pub(super) state: FirmwareReservationState,
    pub(super) expires_at: tokio::time::Instant,
    pub(super) in_flight: bool,
}

#[derive(Debug, Default)]
pub(super) struct FirmwareEntry {
    pub(super) state: StdRwLock<FirmwareEntryState>,
    pub(super) transition: Arc<Mutex<()>>,
    version_observation: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Default)]
pub struct FirmwareObservationCache {
    pub(super) entries: Arc<RwLock<HashMap<String, Arc<FirmwareEntry>>>>,
    // Reverse-session epochs come from the process-wide monotonic NEXT_SESSION_EPOCH counter.
    pub(super) ended_session_epoch: Arc<AtomicU64>,
}

pub struct FirmwareGenerationTransition {
    entry: Arc<FirmwareEntry>,
    serial: String,
    generation: u64,
    _guard: OwnedMutexGuard<()>,
}

impl FirmwareObservationCache {
    #[cfg(test)]
    pub(crate) fn ended_session_epoch_for_test(&self) -> u64 {
        self.ended_session_epoch
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn begin_generation(
        &self,
        config: &AgentConfig,
        endpoint: BambuPrinterEndpoint,
        sender: &mpsc::Sender<AgentEvent>,
        expected_generation: Option<u64>,
    ) -> anyhow::Result<Option<FirmwareGenerationTransition>> {
        let entry = self.entry(&endpoint.serial).await;
        let guard = entry.transition.clone().lock_owned().await;
        let generation = {
            let mut state = entry.state.write().unwrap();
            if expected_generation.is_some_and(|expected| state.generation != expected) {
                return Ok(None);
            }
            state.next_generation = state
                .next_generation
                .checked_add(1)
                .ok_or_else(|| anyhow!("firmware generation overflow for {}", endpoint.serial))?;
            state.generation = state.next_generation;
            state.endpoint = Some(endpoint.clone());
            state.module_revision = 0;
            state.status_revision = 0;
            state.modules = None;
            state.status = None;
            state.reservation = None;
            state.generation
        };
        sender
            .send(firmware_invalidated_event(
                config,
                endpoint.serial.clone(),
                generation,
            ))
            .await
            .with_context(|| {
                format!(
                    "queue printer {} firmware generation {generation} invalidation",
                    endpoint.serial
                )
            })?;
        Ok(Some(FirmwareGenerationTransition {
            entry,
            serial: endpoint.serial,
            generation,
            _guard: guard,
        }))
    }

    pub async fn snapshot(&self, serial: &str) -> Option<FirmwareCacheSnapshot> {
        let entry = self.entries.read().await.get(serial).cloned()?;
        let _guard = entry.transition.clone().lock_owned().await;
        let mut state = entry.state.write().unwrap();
        expire_prepared(&mut state);
        Some(FirmwareCacheSnapshot {
            endpoint: state.endpoint.clone()?,
            generation: state.generation,
            module_revision: state.module_revision,
            status_revision: state.status_revision,
            modules: state.modules.clone(),
            status: state.status.clone(),
            reservation: state
                .reservation
                .as_ref()
                .map(|reservation| reservation.state.clone()),
        })
    }

    #[cfg(test)]
    pub(crate) async fn raw_reservation_for_test(
        &self,
        serial: &str,
    ) -> Option<FirmwareReservationState> {
        let entry = self.entries.read().await.get(serial).cloned()?;
        entry
            .state
            .read()
            .unwrap()
            .reservation
            .as_ref()
            .map(|reservation| reservation.state.clone())
    }

    #[cfg(test)]
    pub(crate) async fn apply_modules_for_test(
        &self,
        observation: FirmwareModulesObservation,
    ) -> bool {
        let entry = self.entry(&observation.serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let mut state = entry.state.write().unwrap();
        if state.generation != observation.generation
            || observation.revision <= state.module_revision
        {
            return false;
        }
        state.module_revision = observation.revision;
        state.modules = Some(observation.modules);
        true
    }

    #[cfg(test)]
    async fn apply_status(&self, observation: FirmwareStatusObservation) -> bool {
        let entry = self.entry(&observation.serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let mut state = entry.state.write().unwrap();
        if state.generation != observation.generation
            || observation.revision <= state.status_revision
        {
            return false;
        }
        state.status_revision = observation.revision;
        state.status = Some(observation.status);
        true
    }

    #[cfg(test)]
    pub(crate) async fn apply_status_for_test(
        &self,
        observation: FirmwareStatusObservation,
    ) -> bool {
        self.apply_status(observation).await
    }

    pub(crate) async fn commit_report_modules(
        &self,
        config: &AgentConfig,
        serial: &str,
        generation: u64,
        modules: Vec<PrinterFirmwareModule>,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<bool> {
        let entry = self.entry(serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let observation = {
            let mut state = entry.state.write().unwrap();
            if state.generation != generation {
                return Ok(false);
            }
            let revision = state
                .module_revision
                .checked_add(1)
                .ok_or_else(|| anyhow!("firmware module revision overflow for {serial}"))?;
            state.module_revision = revision;
            state.modules = Some(modules.clone());
            FirmwareModulesObservation {
                serial: serial.to_owned(),
                generation,
                revision,
                modules,
            }
        };
        #[cfg(test)]
        event_pause::after_commit(serial, event_pause::FirmwareEventKind::Modules).await;
        sender
            .send(firmware_modules_event(config, observation))
            .await
            .with_context(|| format!("queue printer {serial} firmware modules snapshot"))?;
        Ok(true)
    }

    pub(crate) async fn apply_report_status(
        &self,
        config: &AgentConfig,
        observation: FirmwareStatusObservation,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<bool> {
        let entry = self.entry(&observation.serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        {
            let mut state = entry.state.write().unwrap();
            if state.generation != observation.generation
                || observation.revision <= state.status_revision
            {
                return Ok(false);
            }
            state.status_revision = observation.revision;
            state.status = Some(observation.status.clone());
        }
        #[cfg(test)]
        event_pause::after_commit(&observation.serial, event_pause::FirmwareEventKind::Status)
            .await;
        sender
            .send(firmware_status_event(config, observation))
            .await
            .context("queue printer firmware status snapshot")?;
        Ok(true)
    }

    pub async fn commit_modules(
        &self,
        serial: &str,
        generation: u64,
        modules: Vec<PrinterFirmwareModule>,
    ) -> anyhow::Result<Option<FirmwareModulesObservation>> {
        let entry = self.entry(serial).await;
        let _guard = entry.transition.clone().lock_owned().await;
        let mut state = entry.state.write().unwrap();
        if state.generation != generation {
            return Ok(None);
        }
        let revision = state
            .module_revision
            .checked_add(1)
            .ok_or_else(|| anyhow!("firmware module revision overflow for {serial}"))?;
        state.module_revision = revision;
        state.modules = Some(modules.clone());
        Ok(Some(FirmwareModulesObservation {
            serial: serial.to_owned(),
            generation,
            revision,
            modules,
        }))
    }

    pub async fn version_observation_lease(&self, serial: &str) -> OwnedMutexGuard<()> {
        self.entry(serial)
            .await
            .version_observation
            .clone()
            .lock_owned()
            .await
    }

    pub(super) async fn entry(&self, serial: &str) -> Arc<FirmwareEntry> {
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

impl FirmwareGenerationTransition {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub async fn resend_invalidation(
        &self,
        config: &AgentConfig,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        sender
            .send(firmware_invalidated_event(
                config,
                self.serial.clone(),
                self.generation,
            ))
            .await
            .with_context(|| {
                format!(
                    "queue printer {} firmware generation {} invalidation retry",
                    self.serial, self.generation
                )
            })
    }

    pub fn commit_modules(
        &self,
        serial: &str,
        modules: Vec<PrinterFirmwareModule>,
    ) -> anyhow::Result<FirmwareModulesObservation> {
        let mut state = self.entry.state.write().unwrap();
        let revision = state
            .module_revision
            .checked_add(1)
            .ok_or_else(|| anyhow!("firmware module revision overflow for {serial}"))?;
        state.module_revision = revision;
        state.modules = Some(modules.clone());
        Ok(FirmwareModulesObservation {
            serial: serial.to_owned(),
            generation: self.generation,
            revision,
            modules,
        })
    }
}

#[cfg(test)]
pub(crate) mod event_pause;
