use std::{
    io::Write,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{
    callbacks::{FirmwareCallbackQueue, FirmwareTunnel, ReadyFirmwareCallback},
    catalog::firmware_catalog_json,
    http::FirmwareHttpClient,
    model::{FirmwareSendOutcome, FirmwareSendResult, StudioFirmwareParse},
    parser::{PLUGIN_JSON_BODY_LIMIT, parse_studio_firmware},
    status::FirmwareStatusCache,
};
use crate::studio_status::{
    FirmwareProjection, firmware_refresh_failure_json, firmware_refresh_success_json,
};

#[cfg(test)]
mod tests;

pub struct FirmwarePluginSession {
    credentials: Mutex<FirmwareCredentials>,
    status: Mutex<FirmwareStatusCache>,
    callbacks: FirmwareCallbackQueue,
}

struct FirmwareCredentials {
    hub_url: String,
    token: String,
    generation: u64,
    cancelled: bool,
}

struct CredentialsSnapshot {
    hub_url: String,
    token: String,
    generation: u64,
}

impl FirmwarePluginSession {
    pub fn new(hub_url: String, token: String, generation: u64) -> Self {
        Self {
            credentials: Mutex::new(FirmwareCredentials {
                hub_url,
                token,
                generation,
                cancelled: false,
            }),
            status: Mutex::new(FirmwareStatusCache::new(generation)),
            callbacks: FirmwareCallbackQueue::new(),
        }
    }

    pub fn sync_account(&self, hub_url: String, token: String) -> u64 {
        let mut credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        if credentials.hub_url == hub_url && credentials.token == token && !credentials.cancelled {
            return credentials.generation;
        }
        self.advance_generation(&mut credentials, hub_url, token)
    }

    pub fn fence_account(&self, hub_url: String, token: String) -> u64 {
        let mut credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        self.advance_generation(&mut credentials, hub_url, token)
    }

    pub fn generation(&self) -> u64 {
        self.credentials
            .lock()
            .expect("firmware credentials poisoned")
            .generation
    }

    pub fn generation_is_current(&self, expected: u64) -> bool {
        expected == 0 || self.is_current(expected)
    }

    pub fn send(
        &self,
        studio_dev_id: &str,
        printer_id: &str,
        message: &str,
        tunnel: FirmwareTunnel,
        expected_generation: u64,
    ) -> FirmwareSendResult {
        let stderr = std::io::stderr();
        self.send_with_diagnostics(
            studio_dev_id,
            printer_id,
            message,
            tunnel,
            expected_generation,
            &mut stderr.lock(),
        )
    }

    pub fn send_with_diagnostics(
        &self,
        studio_dev_id: &str,
        printer_id: &str,
        message: &str,
        tunnel: FirmwareTunnel,
        expected_generation: u64,
        diagnostics: &mut impl Write,
    ) -> FirmwareSendResult {
        let StudioFirmwareParse::Firmware(command) = parse_studio_firmware(message) else {
            return FirmwareSendResult {
                outcome: FirmwareSendOutcome::PrePublishFailure,
                callback_token: None,
            };
        };
        let Some(snapshot) = self.claim(expected_generation) else {
            return FirmwareSendResult {
                outcome: FirmwareSendOutcome::PrePublishFailure,
                callback_token: None,
            };
        };
        let response = FirmwareHttpClient::new(snapshot.hub_url, snapshot.token).send(
            studio_dev_id,
            printer_id,
            &command,
            tunnel,
            diagnostics,
        );
        let credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        if credentials.generation != expected_generation || credentials.cancelled {
            return FirmwareSendResult {
                outcome: match response.outcome {
                    FirmwareSendOutcome::PrePublishFailure => {
                        FirmwareSendOutcome::PrePublishFailure
                    }
                    _ => FirmwareSendOutcome::OutcomeUnknown,
                },
                callback_token: None,
            };
        }
        let callback_token = response
            .callback
            .and_then(|callback| self.callbacks.push(snapshot.generation, callback));
        FirmwareSendResult {
            outcome: response.outcome,
            callback_token,
        }
    }

    pub fn observe_printers_at(
        &self,
        projection: &FirmwareProjection,
        generation: u64,
        observation_sequence: u64,
        now: Instant,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            projection.source_len() <= PLUGIN_JSON_BODY_LIMIT,
            "printer batch exceeded body limit"
        );
        self.status
            .lock()
            .expect("firmware status cache poisoned")
            .observe_printers_at(projection, generation, observation_sequence, now)
    }

    pub fn observe_printers(
        &self,
        projection: &FirmwareProjection,
        generation: u64,
        observation_sequence: u64,
    ) -> anyhow::Result<()> {
        self.observe_printers_at(projection, generation, observation_sequence, Instant::now())
    }

    pub fn next_status_override_at(&self, dev_id: &str, now: Instant) -> Option<String> {
        self.status
            .lock()
            .expect("firmware status cache poisoned")
            .next_status_override_at(dev_id, now)
    }

    pub fn next_status_override(&self, dev_id: &str) -> Option<String> {
        self.next_status_override_at(dev_id, Instant::now())
    }

    pub fn catalog_json(
        &self,
        studio_dev_id: &str,
        printer_id: &str,
        expected_generation: u64,
    ) -> anyhow::Result<String> {
        let snapshot = self
            .claim(expected_generation)
            .ok_or_else(|| anyhow::anyhow!("firmware session changed"))?;
        let entries =
            FirmwareHttpClient::new(snapshot.hub_url, snapshot.token).catalog(printer_id)?;
        anyhow::ensure!(
            self.is_current(expected_generation),
            "firmware session changed"
        );
        Ok(firmware_catalog_json(studio_dev_id, &entries))
    }

    pub fn refresh_version_json(
        &self,
        printer_id: &str,
        sequence_id: &str,
        expected_generation: u64,
    ) -> String {
        let Some(snapshot) = self.claim(expected_generation) else {
            return firmware_refresh_failure_json(sequence_id);
        };
        let response = FirmwareHttpClient::new(snapshot.hub_url, snapshot.token)
            .refresh(printer_id, sequence_id);
        if !self.is_current(expected_generation) {
            return firmware_refresh_failure_json(sequence_id);
        }
        match response {
            Ok(modules) => firmware_refresh_success_json(sequence_id, &modules),
            Err(error) => {
                eprintln!("pandar firmware version refresh failed: {error:#}");
                firmware_refresh_failure_json(sequence_id)
            }
        }
    }

    pub fn return_handoff_at(
        &self,
        token: u64,
        origin_tick: u64,
        local_generation: u64,
        cache_generation: u64,
        now: Instant,
    ) -> bool {
        self.callbacks.return_handoff_at(
            token,
            origin_tick,
            local_generation,
            cache_generation,
            now,
        )
    }

    pub fn take_ready_callback_at(&self, now: Instant) -> Option<ReadyFirmwareCallback> {
        self.callbacks.take_ready_at(now)
    }

    pub fn wait_ready_callback(&self, timeout: Duration) -> Option<ReadyFirmwareCallback> {
        self.callbacks.wait_ready(timeout)
    }

    pub fn cancel_generation(&self, generation: u64) {
        let mut credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        if credentials.generation == generation {
            credentials.cancelled = true;
        }
        self.callbacks.cancel_generation(generation);
    }

    pub fn stop(&self) {
        self.callbacks.stop();
    }

    fn advance_generation(
        &self,
        credentials: &mut FirmwareCredentials,
        hub_url: String,
        token: String,
    ) -> u64 {
        let previous_generation = credentials.generation;
        self.callbacks.cancel_generation(previous_generation);
        credentials.generation = credentials.generation.wrapping_add(1);
        credentials.hub_url = hub_url;
        credentials.token = token;
        credentials.cancelled = false;
        self.status
            .lock()
            .expect("firmware status cache poisoned")
            .update_generation(credentials.generation, Instant::now());
        credentials.generation
    }

    fn claim(&self, expected_generation: u64) -> Option<CredentialsSnapshot> {
        let credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        if credentials.generation != expected_generation || credentials.cancelled {
            return None;
        }
        Some(CredentialsSnapshot {
            hub_url: credentials.hub_url.clone(),
            token: credentials.token.clone(),
            generation: credentials.generation,
        })
    }

    fn is_current(&self, expected_generation: u64) -> bool {
        let credentials = self
            .credentials
            .lock()
            .expect("firmware credentials poisoned");
        credentials.generation == expected_generation && !credentials.cancelled
    }
}

impl Drop for FirmwarePluginSession {
    fn drop(&mut self) {
        self.callbacks.stop();
    }
}
