use super::*;

impl ConnectionSession {
    fn fail_printer_refresh(
        &self,
        snapshot: &RequestSnapshot,
        transport_failure: bool,
        auth_rejected: bool,
        auth_response_reachable: bool,
    ) {
        let mut state = self.state.lock().expect("connection state");
        if !snapshot.is_current(&state) {
            return;
        }
        if !auth_rejected || !auth_response_reachable {
            state.fail_unconfirmed_online();
        }
        state.printers_fresh = false;
        state.studio.invalidate_cache();
        if transport_failure {
            state.set_reachability(Reachability::Disconnected);
        }
        if auth_response_reachable {
            state.set_reachability(Reachability::Connected);
        }
        if auth_rejected {
            state.reject_auth();
        }
    }

    fn commit_printers(
        &self,
        snapshot: &RequestSnapshot,
        printers: Vec<PrinterObservation>,
    ) -> bool {
        let mut state = self.state.lock().expect("connection state");
        if !snapshot.is_current(&state) {
            return false;
        }
        let next = printers
            .into_iter()
            .map(|printer| (printer.dev_id.clone(), printer))
            .collect::<BTreeMap<_, _>>();
        let confirmed_online = next
            .values()
            .filter(|printer| printer.online)
            .map(|printer| printer.dev_id.clone())
            .collect::<BTreeSet<_>>();
        state.reconcile_online(&confirmed_online);
        let identity_changed = state.printers.len() != next.len()
            || state.printers.iter().any(|(dev_id, previous)| {
                next.get(dev_id).is_none_or(|current| {
                    current.pandar_printer_id != previous.pandar_printer_id
                        || current.model != previous.model
                })
            });
        state.printers = next;
        if identity_changed {
            state.studio.invalidate_cache();
        }
        state.printers_fresh = true;
        state.accept_auth();
        state.set_reachability(Reachability::Connected);
        true
    }

    pub(super) fn refresh_printers(
        &self,
        expected: Option<(&str, &str, u64)>,
        invalidate_freshness: bool,
        reserve_observation: impl FnOnce(),
    ) -> PrinterRefreshResult {
        let Ok(_request) = self.printer_request.try_lock() else {
            return PrinterRefreshResult::without_firmware(result(
                1,
                0,
                stable_error_body("hub_unavailable"),
            ));
        };
        let Some(snapshot) = self.begin_printer_refresh(expected, invalidate_freshness) else {
            return PrinterRefreshResult::without_firmware(result(
                1,
                409,
                stable_error_body("stale_no_auth_session"),
            ));
        };
        if snapshot.token.trim().is_empty() {
            self.fail_printer_refresh(&snapshot, false, true, false);
            return PrinterRefreshResult::without_firmware(result(
                1,
                400,
                stable_error_body("invalid_auth_token"),
            ));
        }

        let response = match fetch_printers(&snapshot, reserve_observation) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("pandar printer status refresh failed: {error:#}");
                self.fail_printer_refresh(&snapshot, true, false, false);
                return PrinterRefreshResult::without_firmware(result(
                    1,
                    0,
                    stable_error_body("hub_unavailable"),
                ));
            }
        };
        if !(200..300).contains(&response.http_code) {
            let auth_rejected = matches!(response.http_code, 401 | 403);
            self.fail_printer_refresh(&snapshot, false, auth_rejected, auth_rejected);
            return PrinterRefreshResult::without_firmware(result(
                1,
                response.http_code,
                http::redact_hub_error(
                    RequestKind::PrinterLookup,
                    response.http_code,
                    &response.body,
                ),
            ));
        }
        let projection = match project_hub_printers(&response.body) {
            Ok(projection) => projection,
            Err(error) => {
                eprintln!(
                    "pandar printer status refresh failed: {:#}",
                    error.context("validate Hub printer status refresh response")
                );
                self.fail_printer_refresh(&snapshot, false, false, false);
                return PrinterRefreshResult::without_firmware(result(
                    1,
                    response.http_code,
                    stable_error_body("invalid_response"),
                ));
            }
        };
        let (printers, firmware) = projection.into_parts();
        if !self.commit_printers(&snapshot, printers) {
            eprintln!(
                "pandar printer status refresh discarded: credentials changed during request"
            );
            return PrinterRefreshResult::without_firmware(result(
                1,
                0,
                stable_error_body("hub_unavailable"),
            ));
        }
        PrinterRefreshResult::projected(result(0, response.http_code, response.body), firmware)
    }
}
