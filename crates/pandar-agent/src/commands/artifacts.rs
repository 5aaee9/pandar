use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use futures_util::StreamExt;

use crate::AgentConfig;

#[async_trait]
pub trait ArtifactReader: Send + Sync {
    async fn read_artifact(&self, artifact_download_path: &str) -> anyhow::Result<Vec<u8>>;
}

pub struct HubArtifactReader {
    hub_api_url: Option<String>,
    hub_grpc_url: String,
    agent_credential: String,
    client: reqwest::Client,
}

impl HubArtifactReader {
    pub fn new(config: &AgentConfig) -> Self {
        Self {
            hub_api_url: config.hub_api_url.clone(),
            hub_grpc_url: config.hub_grpc_url.clone(),
            agent_credential: config.agent_credential.clone(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("hub artifact HTTP client configuration is valid"),
        }
    }
}

#[async_trait]
impl ArtifactReader for HubArtifactReader {
    async fn read_artifact(&self, artifact_download_path: &str) -> anyhow::Result<Vec<u8>> {
        let url = artifact_download_url(
            self.hub_api_url.as_deref(),
            &self.hub_grpc_url,
            artifact_download_path,
        )?;
        let response = self
            .client
            .get(url.clone())
            .bearer_auth(&self.agent_credential)
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .context("request print artifact from hub")?;
        let status = response.status();
        if !status.is_success() {
            bail!("hub artifact download failed with HTTP {status}");
        }

        const MAX_ARTIFACT_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARTIFACT_RESPONSE_BYTES as u64)
        {
            bail!("hub artifact exceeds {MAX_ARTIFACT_RESPONSE_BYTES} bytes");
        }
        let mut artifact = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(reqwest::Error::without_url)
                .context("read print artifact response from hub")?;
            if artifact.len().saturating_add(chunk.len()) > MAX_ARTIFACT_RESPONSE_BYTES {
                bail!("hub artifact exceeds {MAX_ARTIFACT_RESPONSE_BYTES} bytes");
            }
            artifact.extend_from_slice(&chunk);
        }
        Ok(artifact)
    }
}

pub fn artifact_download_url(
    configured_hub_api_url: Option<&str>,
    hub_grpc_url: &str,
    artifact_download_path: &str,
) -> anyhow::Result<String> {
    let base = match configured_hub_api_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(url) => url.to_owned(),
        None if hub_grpc_url.starts_with("http://") || hub_grpc_url.starts_with("https://") => {
            hub_grpc_url.to_owned()
        }
        None => bail!(
            "PANDAR_HUB_API_URL is required to download hub artifacts when PANDAR_HUB_GRPC_URL is not http:// or https://"
        ),
    };
    Ok(format!(
        "{}/{}",
        base.trim_end_matches('/'),
        artifact_download_path.trim_start_matches('/')
    ))
}
