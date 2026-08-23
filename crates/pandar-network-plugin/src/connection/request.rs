use std::time::Duration;

use anyhow::Context;

use crate::runtime;

const HEALTH_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct RequestSnapshot {
    pub(super) hub_url: String,
    pub(super) generation: u64,
    pub(super) account_epoch: u64,
}

pub(super) fn fetch_readiness(snapshot: &RequestSnapshot) -> anyhow::Result<HubResponse> {
    fetch(snapshot, "/healthz", None).context("refresh Hub readiness")
}

pub(super) struct HubResponse {
    pub(super) http_code: u32,
    pub(super) body: String,
}

fn fetch(
    snapshot: &RequestSnapshot,
    path: &str,
    token: Option<&str>,
) -> anyhow::Result<HubResponse> {
    runtime().block_on(async move {
        tokio::time::timeout(HEALTH_REQUEST_TIMEOUT, async move {
            let client = reqwest::Client::builder()
                .connect_timeout(HEALTH_REQUEST_TIMEOUT)
                .timeout(HEALTH_REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .context("build Hub connection client")?;
            let request = client.get(format!("{}{path}", snapshot.hub_url));
            let request = match token {
                Some(token) => request.bearer_auth(token),
                None => request,
            };
            let response = request
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .context("send Hub connection request")?;
            let http_code = response.status().as_u16().into();
            let body = crate::http::read_bounded_response_body(response)
                .await
                .context("read Hub connection response")?;
            Ok(HubResponse { http_code, body })
        })
        .await
        .context("Hub connection request timed out")?
    })
}
