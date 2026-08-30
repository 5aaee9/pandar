use std::{sync::OnceLock, time::Duration};

use anyhow::Context;

const HUB_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn hub_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(HUB_HTTP_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .expect("Hub HTTP client configuration is valid")
    })
}

pub(crate) async fn send_hub_request(
    request: reqwest::RequestBuilder,
    context: &'static str,
) -> anyhow::Result<reqwest::Response> {
    request
        .send()
        .await
        .map_err(reqwest::Error::without_url)
        .context(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_client_is_reused() {
        assert!(std::ptr::eq(hub_client(), hub_client()));
    }
}
