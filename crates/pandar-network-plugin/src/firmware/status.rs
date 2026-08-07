use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use pandar_core::PrinterFirmwareState;

use crate::studio_status::{FirmwareProjection, current_firmware_json, firmware_reset_json};

const RESET_GUARD: Duration = Duration::from_secs(3);

pub struct FirmwareStatusCache {
    generation: u64,
    last_observation_sequence: Option<u64>,
    devices: HashMap<String, DeviceFirmwareStatus>,
}

struct DeviceFirmwareStatus {
    identity: FirmwareIdentity,
    latest: PrinterFirmwareState,
    presentation: FirmwarePresentation,
}

#[derive(Eq, PartialEq)]
struct FirmwareIdentity {
    session_id: String,
    generation: u64,
}

enum FirmwarePresentation {
    Unavailable,
    Current,
    Resetting {
        schedule: ResetSchedule,
        pending_current: bool,
    },
}

struct ResetSchedule {
    started_at: Instant,
    emitted: bool,
    emitted_after_guard: bool,
}

impl FirmwareStatusCache {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            last_observation_sequence: None,
            devices: HashMap::new(),
        }
    }

    pub fn observe_printers_at(
        &mut self,
        projection: &FirmwareProjection,
        generation: u64,
        observation_sequence: u64,
        now: Instant,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            generation == self.generation,
            "stale plugin firmware generation"
        );
        if self
            .last_observation_sequence
            .is_some_and(|last| observation_sequence <= last)
        {
            return Ok(());
        }
        let observed_ids = projection
            .observations()
            .iter()
            .map(|printer| printer.dev_id.clone())
            .collect::<HashSet<_>>();

        for observation in projection.observations() {
            match &observation.firmware {
                Some(firmware) => self.observe(&observation.dev_id, firmware.clone(), now),
                None => self.invalidate(&observation.dev_id, now),
            }
        }
        for dev_id in self.devices.keys().cloned().collect::<Vec<_>>() {
            if !observed_ids.contains(&dev_id) {
                self.invalidate(&dev_id, now);
            }
        }
        self.last_observation_sequence = Some(observation_sequence);
        Ok(())
    }

    pub fn next_status_override_at(&mut self, dev_id: &str, now: Instant) -> Option<String> {
        let status = self.devices.get_mut(dev_id)?;
        let ready_current = matches!(
            status.presentation,
            FirmwarePresentation::Resetting {
                ref schedule,
                pending_current: true,
            } if schedule.emitted
        );
        if ready_current {
            status.presentation = FirmwarePresentation::Current;
        }
        match &mut status.presentation {
            FirmwarePresentation::Unavailable => None,
            FirmwarePresentation::Current => current_firmware_json(&status.latest),
            FirmwarePresentation::Resetting { schedule, .. } => {
                schedule.next_at(now).then(firmware_reset_json)
            }
        }
    }

    pub(super) fn update_generation(&mut self, generation: u64, now: Instant) {
        if self.generation == generation {
            return;
        }
        self.generation = generation;
        for status in self.devices.values_mut() {
            status.invalidate(now);
        }
    }

    fn invalidate(&mut self, dev_id: &str, now: Instant) {
        let Some(status) = self.devices.get_mut(dev_id) else {
            return;
        };
        status.invalidate(now);
    }

    fn observe(&mut self, dev_id: &str, firmware: PrinterFirmwareState, now: Instant) {
        let Some(identity) = FirmwareIdentity::from_state(&firmware) else {
            self.invalidate(dev_id, now);
            return;
        };
        let Some(status) = self.devices.get_mut(dev_id) else {
            let presentation = if has_current_payload(&firmware) {
                FirmwarePresentation::Current
            } else {
                FirmwarePresentation::Unavailable
            };
            self.devices.insert(
                dev_id.to_owned(),
                DeviceFirmwareStatus {
                    identity,
                    latest: firmware,
                    presentation,
                },
            );
            return;
        };

        if identity.session_id == status.identity.session_id {
            match identity.generation.cmp(&status.identity.generation) {
                Ordering::Less => return,
                Ordering::Greater => status.replace_identity(identity, firmware, now),
                Ordering::Equal => {
                    merge_same_identity(&mut status.latest, firmware);
                    status.refresh_presentation(now);
                }
            }
            return;
        }
        status.replace_identity(identity, firmware, now);
    }
}

impl FirmwareIdentity {
    fn from_state(state: &PrinterFirmwareState) -> Option<Self> {
        Some(Self {
            session_id: state.session_id.clone()?,
            generation: state.generation?,
        })
    }
}

impl DeviceFirmwareStatus {
    fn replace_identity(
        &mut self,
        identity: FirmwareIdentity,
        firmware: PrinterFirmwareState,
        now: Instant,
    ) {
        let pending_current = has_current_payload(&firmware);
        self.identity = identity;
        self.latest = firmware;
        match &mut self.presentation {
            FirmwarePresentation::Unavailable => {
                if pending_current {
                    self.presentation = FirmwarePresentation::Current;
                }
            }
            FirmwarePresentation::Current => {
                self.presentation = FirmwarePresentation::Resetting {
                    schedule: ResetSchedule::new(now),
                    pending_current,
                };
            }
            FirmwarePresentation::Resetting {
                pending_current: pending,
                ..
            } => *pending = pending_current,
        }
    }

    fn refresh_presentation(&mut self, now: Instant) {
        let pending_current = has_current_payload(&self.latest);
        match &mut self.presentation {
            FirmwarePresentation::Unavailable => {
                if pending_current {
                    self.presentation = FirmwarePresentation::Current;
                }
            }
            FirmwarePresentation::Current if !pending_current => {
                self.presentation = FirmwarePresentation::Resetting {
                    schedule: ResetSchedule::new(now),
                    pending_current: false,
                };
            }
            FirmwarePresentation::Resetting {
                pending_current: pending,
                ..
            } => *pending = pending_current,
            FirmwarePresentation::Current => {}
        }
    }

    fn invalidate(&mut self, now: Instant) {
        match &mut self.presentation {
            FirmwarePresentation::Unavailable => {}
            FirmwarePresentation::Current => {
                self.presentation = FirmwarePresentation::Resetting {
                    schedule: ResetSchedule::new(now),
                    pending_current: false,
                };
            }
            FirmwarePresentation::Resetting {
                pending_current, ..
            } => *pending_current = false,
        }
    }
}

impl ResetSchedule {
    fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            emitted: false,
            emitted_after_guard: false,
        }
    }

    fn next_at(&mut self, now: Instant) -> bool {
        if self.emitted_after_guard {
            return false;
        }
        self.emitted = true;
        if now.saturating_duration_since(self.started_at) >= RESET_GUARD {
            self.emitted_after_guard = true;
        }
        true
    }
}

fn merge_same_identity(current: &mut PrinterFirmwareState, incoming: PrinterFirmwareState) {
    if incoming.module_revision > current.module_revision {
        current.module_revision = incoming.module_revision;
        current.modules = incoming.modules;
    }
    if incoming.status_revision > current.status_revision {
        current.status_revision = incoming.status_revision;
        current.upgrade_state = incoming.upgrade_state;
        current.cfg = incoming.cfg;
    }
}

fn has_current_payload(state: &PrinterFirmwareState) -> bool {
    state.modules.is_some() || state.upgrade_state.is_some() || state.cfg.is_some()
}
