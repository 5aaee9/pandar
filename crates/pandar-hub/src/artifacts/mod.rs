use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use anyhow::{Context, bail};
use pandar_core::TenantId;
use tokio::{fs, io::AsyncRead};

mod app_state;
mod filesystem;
pub(crate) mod metadata;
mod s3;

pub use app_state::{IntoArtifactStorage, JobStorageAlias};
pub use filesystem::{FilesystemArtifactStorage, sanitize_filename};
pub use s3::{S3ArtifactStorage, S3ArtifactStorageConfig};

pub const DEFAULT_MAX_ARTIFACT_BYTES: usize = 10_485_760;

pub type ArtifactBody = fs::File;

pub struct ArtifactUploadBody<'a> {
    pub(super) reader: Pin<Box<dyn AsyncRead + Send + 'a>>,
}

impl<'a> ArtifactUploadBody<'a> {
    pub fn reader(reader: impl AsyncRead + Send + 'a) -> Self {
        Self {
            reader: Box::pin(reader),
        }
    }
}

pub struct StoreArtifactInput<'a> {
    pub tenant_id: TenantId,
    pub artifact_id: &'a str,
    pub filename: &'a str,
    pub body: ArtifactUploadBody<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactStorageBackend {
    Filesystem,
    S3,
}

impl ArtifactStorageBackend {
    pub fn requires_hub_fetch(self) -> bool {
        match self {
            Self::Filesystem => false,
            Self::S3 => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub filename: String,
    pub storage_key: String,
    pub storage_path: String,
    pub size_bytes: u64,
    pub backend: ArtifactStorageBackend,
}

#[async_trait::async_trait]
pub trait ArtifactStorage: Send + Sync {
    async fn put_artifact(&self, input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact>;
    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody>;
    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()>;
    async fn check_ready(&self) -> anyhow::Result<()>;
    fn max_artifact_bytes(&self) -> usize;
    fn backend(&self) -> ArtifactStorageBackend;
    fn is_not_found(&self, _err: &anyhow::Error) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct ArtifactStorageConfig {
    backend: ArtifactStorageBackend,
    spool_dir: Option<PathBuf>,
    s3: Option<S3ArtifactStorageConfig>,
    max_artifact_bytes: usize,
}

impl ArtifactStorageConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_env_values(
            std::env::var("PANDAR_ARTIFACT_STORAGE").ok(),
            std::env::var("PANDAR_SPOOL_DIR").ok(),
            match std::env::var("PANDAR_MAX_ARTIFACT_BYTES") {
                Ok(value) => Some(value),
                Err(std::env::VarError::NotPresent) => None,
                Err(err) => return Err(err).context("failed to read PANDAR_MAX_ARTIFACT_BYTES"),
            },
        )
    }

    pub fn from_env_values(
        backend: Option<impl AsRef<str>>,
        spool_dir: Option<impl Into<PathBuf>>,
        max_artifact_bytes: Option<impl AsRef<str>>,
    ) -> anyhow::Result<Self> {
        let backend = match backend.as_ref().map(|value| value.as_ref().trim()) {
            None | Some("") | Some("filesystem") => ArtifactStorageBackend::Filesystem,
            Some("s3") => ArtifactStorageBackend::S3,
            Some(other) => bail!("unsupported PANDAR_ARTIFACT_STORAGE value {other}"),
        };
        let max_artifact_bytes = match max_artifact_bytes {
            Some(value) => value
                .as_ref()
                .parse::<usize>()
                .context("failed to parse PANDAR_MAX_ARTIFACT_BYTES")?,
            None => DEFAULT_MAX_ARTIFACT_BYTES,
        };
        validate_max_artifact_bytes(max_artifact_bytes)?;

        Ok(Self {
            backend,
            spool_dir: match backend {
                ArtifactStorageBackend::Filesystem => Some(
                    spool_dir
                        .map(Into::into)
                        .unwrap_or_else(|| PathBuf::from("pandar-spool")),
                ),
                ArtifactStorageBackend::S3 => None,
            },
            s3: None,
            max_artifact_bytes,
        })
    }

    pub fn backend(&self) -> ArtifactStorageBackend {
        self.backend
    }

    pub fn max_artifact_bytes(&self) -> usize {
        self.max_artifact_bytes
    }

    pub fn spool_dir(&self) -> Option<&Path> {
        self.spool_dir.as_deref()
    }

    pub async fn build(&self) -> anyhow::Result<Arc<dyn ArtifactStorage>> {
        match self.backend {
            ArtifactStorageBackend::Filesystem => Ok(Arc::new(FilesystemArtifactStorage::new(
                self.spool_dir
                    .as_ref()
                    .expect("filesystem artifact storage requires a spool directory"),
                self.max_artifact_bytes,
            )?)),
            ArtifactStorageBackend::S3 => {
                let config = match &self.s3 {
                    Some(config) => config.clone(),
                    None => S3ArtifactStorageConfig::from_env()?,
                };
                Ok(Arc::new(config.build().await?))
            }
        }
    }
}

fn validate_max_artifact_bytes(max_artifact_bytes: usize) -> anyhow::Result<()> {
    if max_artifact_bytes == 0 {
        bail!("PANDAR_MAX_ARTIFACT_BYTES must be greater than zero");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingUploadReader {
        emitted_prefix: bool,
    }

    impl tokio::io::AsyncRead for FailingUploadReader {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if self.emitted_prefix {
                return std::task::Poll::Ready(Err(std::io::Error::other("upload stream failed")));
            }

            self.emitted_prefix = true;
            buf.put_slice(b"partial");
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn filesystem_storage_rejects_empty_and_oversized_upload() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FilesystemArtifactStorage::new(temp_dir.path(), 4).unwrap();

        let empty = StoreArtifactInput {
            tenant_id: pandar_core::TenantId::new(),
            artifact_id: "artifact",
            filename: "plate.3mf",
            body: ArtifactUploadBody::reader(tokio::io::empty()),
        };
        assert!(storage.put_artifact(empty).await.is_err());

        let oversized = StoreArtifactInput {
            tenant_id: pandar_core::TenantId::new(),
            artifact_id: "artifact",
            filename: "plate.3mf",
            body: ArtifactUploadBody::reader(std::io::Cursor::new(b"12345".to_vec())),
        };
        assert!(storage.put_artifact(oversized).await.is_err());
    }

    #[tokio::test]
    async fn filesystem_storage_removes_partial_file_when_upload_reader_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FilesystemArtifactStorage::new(temp_dir.path(), 1024).unwrap();
        let tenant_id = pandar_core::TenantId::new();

        let result = storage
            .put_artifact(StoreArtifactInput {
                tenant_id,
                artifact_id: "artifact",
                filename: "plate.3mf",
                body: ArtifactUploadBody::reader(FailingUploadReader {
                    emitted_prefix: false,
                }),
            })
            .await;

        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("upload stream failed"));
        assert!(
            !temp_dir
                .path()
                .join(tenant_id.to_string())
                .join("artifact")
                .join("plate.3mf")
                .try_exists()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn filesystem_storage_sanitizes_filename_and_returns_opaque_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FilesystemArtifactStorage::new(temp_dir.path(), 1024).unwrap();
        let tenant_id = pandar_core::TenantId::new();

        let artifact = storage
            .put_artifact(StoreArtifactInput {
                tenant_id,
                artifact_id: "artifact",
                filename: "../plate file.3mf",
                body: ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
            })
            .await
            .unwrap();

        assert_eq!(artifact.filename, "plate_file.3mf");
        assert_eq!(artifact.size_bytes, 3);
        assert_ne!(artifact.storage_key, "../plate_file.3mf");
        assert!(
            temp_dir
                .path()
                .join(&artifact.storage_key)
                .try_exists()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn filesystem_storage_open_and_delete_round_trip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FilesystemArtifactStorage::new(temp_dir.path(), 1024).unwrap();

        let artifact = storage
            .put_artifact(StoreArtifactInput {
                tenant_id: pandar_core::TenantId::new(),
                artifact_id: "artifact",
                filename: "plate.3mf",
                body: ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
            })
            .await
            .unwrap();

        let mut body = storage.open_artifact(&artifact.storage_key).await.unwrap();
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
            .await
            .unwrap();
        assert_eq!(bytes, b"abc");

        storage
            .delete_artifact(&artifact.storage_key)
            .await
            .unwrap();
        assert!(storage.open_artifact(&artifact.storage_key).await.is_err());
        storage
            .delete_artifact(&artifact.storage_key)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn filesystem_storage_rejects_unsafe_key_on_read_and_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FilesystemArtifactStorage::new(temp_dir.path(), 1024).unwrap();

        assert!(storage.open_artifact("../escape").await.is_err());
        assert!(storage.open_artifact("/tmp/escape").await.is_err());
        assert!(storage.delete_artifact("../escape").await.is_err());
        assert!(storage.delete_artifact("/tmp/escape").await.is_err());
    }

    #[test]
    fn artifact_storage_config_defaults_to_filesystem() {
        let config = ArtifactStorageConfig::from_env_values(
            None::<&str>,
            None::<&std::path::Path>,
            None::<&str>,
        )
        .unwrap();

        assert_eq!(config.backend(), ArtifactStorageBackend::Filesystem);
        assert!(!config.backend().requires_hub_fetch());
        assert_eq!(config.max_artifact_bytes(), DEFAULT_MAX_ARTIFACT_BYTES);
        assert_eq!(
            config.spool_dir().unwrap(),
            std::path::Path::new("pandar-spool")
        );
    }
}
