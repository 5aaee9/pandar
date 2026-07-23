use std::time::Duration;

use anyhow::Context;

use crate::runtime;

use super::ConnectionSession;

const REQUEST_TIMEOUT: Duration = Duration::from_millis(750);

pub(super) struct RequestSnapshot {
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) generation: u64,
    pub(super) printer_epoch: u64,
}

pub(super) struct HubResponse {
    pub(super) http_code: u32,
    pub(super) body: String,
}

impl ConnectionSession {
    pub(super) fn begin_printer_refresh(
        &self,
        expected: Option<(&str, &str, u64)>,
        invalidate_freshness: bool,
    ) -> Option<RequestSnapshot> {
        let mut state = self.state.lock().expect("connection state");
        if expected.is_some_and(|(hub_url, token, account_epoch)| {
            state.hub_url != hub_url || state.token != token || state.account_epoch != account_epoch
        }) {
            return None;
        }
        state.capture_online();
        state.printer_epoch = state.printer_epoch.wrapping_add(1);
        if invalidate_freshness {
            state.printers_fresh = false;
        }
        Some(RequestSnapshot {
            hub_url: state.hub_url.clone(),
            token: state.token.clone(),
            generation: state.generation,
            printer_epoch: state.printer_epoch,
        })
    }
}

pub(super) fn fetch_readiness(snapshot: &RequestSnapshot) -> anyhow::Result<HubResponse> {
    fetch(snapshot, "/readyz", None, || {}).context("refresh Hub readiness")
}

pub(super) fn fetch_printers(
    snapshot: &RequestSnapshot,
    reserve_observation: impl FnOnce(),
) -> anyhow::Result<HubResponse> {
    fetch(
        snapshot,
        "/api/v1/plugin/printers",
        Some(&snapshot.token),
        reserve_observation,
    )
    .context("refresh Hub printer status")
}

fn fetch(
    snapshot: &RequestSnapshot,
    path: &str,
    token: Option<&str>,
    before_send: impl FnOnce(),
) -> anyhow::Result<HubResponse> {
    runtime().block_on(async move {
        tokio::time::timeout(REQUEST_TIMEOUT, async move {
            let client = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .context("build Hub connection client")?;
            let request = client.get(format!("{}{path}", snapshot.hub_url));
            let request = match token {
                Some(token) => request.bearer_auth(token),
                None => request,
            };
            before_send();
            let response = request
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .context("send Hub connection request")?;
            let http_code = response.status().as_u16().into();
            let body = response
                .text()
                .await
                .map_err(reqwest::Error::without_url)
                .context("read Hub connection response")?;
            Ok(HubResponse { http_code, body })
        })
        .await
        .context("Hub connection request timed out")?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_fresh_cache() -> ConnectionSession {
        let session = ConnectionSession::new("http://127.0.0.1:1".into(), "token".into());
        session
            .state
            .lock()
            .expect("connection state")
            .printers_fresh = true;
        session
    }

    #[test]
    fn background_refresh_preserves_last_confirmed_cache_while_in_flight() {
        let session = session_with_fresh_cache();

        session
            .begin_printer_refresh(None, false)
            .expect("background refresh snapshot");

        assert!(
            session
                .state
                .lock()
                .expect("connection state")
                .printers_fresh
        );
    }

    #[test]
    fn foreground_refresh_invalidates_cache_while_in_flight() {
        let session = session_with_fresh_cache();

        session
            .begin_printer_refresh(None, true)
            .expect("foreground refresh snapshot");

        assert!(
            !session
                .state
                .lock()
                .expect("connection state")
                .printers_fresh
        );
    }
}
