use anyhow::Context;

use crate::AgentConfig;

pub(super) fn validate_hub_transport_urls(config: &AgentConfig) -> anyhow::Result<()> {
    validate_hub_transport_url("PANDAR_HUB_GRPC_URL", &config.hub_grpc_url)?;
    if let Some(url) = config.hub_api_url.as_deref() {
        validate_hub_transport_url("PANDAR_HUB_API_URL", url)?;
    }
    Ok(())
}

fn validate_hub_transport_url(name: &str, value: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(value).with_context(|| format!("invalid {name}"))?;
    if url.scheme() != "http" {
        return Ok(());
    }
    let host = url.host_str().unwrap_or_default();
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !loopback {
        anyhow::bail!("{name} must use https for a non-loopback host");
    }
    Ok(())
}
