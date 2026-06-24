use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use async_trait::async_trait;

use crate::{AgentConfig, protocol::agent::v1::PrintProjectFile};

#[async_trait]
pub trait ArtifactReader: Send + Sync {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>>;
}

pub struct FilesystemArtifactReader {
    root: PathBuf,
}

impl FilesystemArtifactReader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl ArtifactReader for FilesystemArtifactReader {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>> {
        let artifact_path = resolve_artifact_path(&self.root, storage_path)?;
        tokio::task::spawn_blocking(move || std::fs::read(&artifact_path))
            .await
            .context("join print artifact read task")?
            .with_context(|| format!("read print artifact {storage_path}"))
    }
}

pub struct CommandArtifactReader {
    local: FilesystemArtifactReader,
    hub: HubArtifactReader,
}

impl CommandArtifactReader {
    pub fn new(config: &AgentConfig) -> Self {
        Self {
            local: FilesystemArtifactReader::new(config.artifact_root.clone()),
            hub: HubArtifactReader::new(config),
        }
    }

    pub async fn read_print_artifact(
        &self,
        command: &crate::protocol::agent::v1::PrintProjectFile,
    ) -> anyhow::Result<Vec<u8>> {
        if command.artifact_download_path.trim().is_empty() {
            return self.local.read_artifact(&command.storage_path).await;
        }

        self.hub
            .read_artifact(&command.artifact_download_path)
            .await
            .context("download print artifact from hub")
    }
}

#[async_trait]
pub trait PrintCommandArtifactReader: Send + Sync {
    async fn read_print_artifact(&self, command: &PrintProjectFile) -> anyhow::Result<Vec<u8>>;
}

#[async_trait]
impl PrintCommandArtifactReader for CommandArtifactReader {
    async fn read_print_artifact(&self, command: &PrintProjectFile) -> anyhow::Result<Vec<u8>> {
        self.read_print_artifact(command).await
    }
}

pub struct LegacyCommandArtifactReader<'a, R> {
    pub artifact_reader: &'a R,
}

#[async_trait]
impl<R> PrintCommandArtifactReader for LegacyCommandArtifactReader<'_, R>
where
    R: ArtifactReader,
{
    async fn read_print_artifact(&self, command: &PrintProjectFile) -> anyhow::Result<Vec<u8>> {
        self.artifact_reader
            .read_artifact(&command.storage_path)
            .await
    }
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
            client: reqwest::Client::new(),
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
            .context("request print artifact from hub")?;
        let status = response.status();
        if !status.is_success() {
            bail!("hub artifact download failed with HTTP {status}");
        }

        Ok(response
            .bytes()
            .await
            .context("read print artifact response from hub")?
            .to_vec())
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

pub fn resolve_artifact_path(root: &Path, storage_path: &str) -> anyhow::Result<PathBuf> {
    let storage_path = Path::new(storage_path);
    if storage_path.is_absolute() {
        bail!("artifact storage path must be relative");
    }
    if storage_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("artifact storage path must not contain parent or prefix components");
    }

    Ok(root.join(storage_path))
}
