use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, bail};
use pandar_core::TenantId;
use tokio::{fs, io::AsyncWriteExt};

pub const DEFAULT_MAX_ARTIFACT_BYTES: usize = 10_485_760;

#[derive(Debug, Clone)]
pub struct JobStorageConfig {
    spool_dir: Arc<PathBuf>,
    max_artifact_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub filename: String,
    pub storage_path: String,
    pub size_bytes: u64,
}

impl JobStorageConfig {
    pub fn new(spool_dir: impl Into<PathBuf>, max_artifact_bytes: usize) -> anyhow::Result<Self> {
        if max_artifact_bytes == 0 {
            bail!("PANDAR_MAX_ARTIFACT_BYTES must be greater than zero");
        }

        Ok(Self {
            spool_dir: Arc::new(spool_dir.into()),
            max_artifact_bytes,
        })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let spool_dir =
            std::env::var("PANDAR_SPOOL_DIR").unwrap_or_else(|_| "pandar-spool".to_string());
        let max_artifact_bytes = match std::env::var("PANDAR_MAX_ARTIFACT_BYTES") {
            Ok(value) => value
                .parse::<usize>()
                .context("failed to parse PANDAR_MAX_ARTIFACT_BYTES")?,
            Err(std::env::VarError::NotPresent) => DEFAULT_MAX_ARTIFACT_BYTES,
            Err(err) => return Err(err).context("failed to read PANDAR_MAX_ARTIFACT_BYTES"),
        };

        Self::new(spool_dir, max_artifact_bytes)
    }

    pub fn max_artifact_bytes(&self) -> usize {
        self.max_artifact_bytes
    }

    pub fn spool_dir(&self) -> &Path {
        &self.spool_dir
    }

    pub async fn write_artifact(
        &self,
        tenant_id: TenantId,
        artifact_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> anyhow::Result<StoredArtifact> {
        if bytes.is_empty() {
            bail!("artifact bytes cannot be empty");
        }
        if bytes.len() > self.max_artifact_bytes {
            bail!(
                "artifact is larger than configured maximum of {} bytes",
                self.max_artifact_bytes
            );
        }

        let filename = sanitize_filename(filename);
        let storage_path = PathBuf::from(tenant_id.to_string())
            .join(artifact_id)
            .join(&filename);
        let path = self.spool_dir.join(&storage_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("failed to create artifact directory {}", parent.display())
            })?;
        }

        let mut file = fs::File::create(&path)
            .await
            .with_context(|| format!("failed to create artifact file {}", path.display()))?;
        file.write_all(bytes)
            .await
            .with_context(|| format!("failed to write artifact file {}", path.display()))?;

        Ok(StoredArtifact {
            filename,
            storage_path: storage_path.to_string_lossy().into_owned(),
            size_bytes: bytes.len() as u64,
        })
    }

    pub async fn remove_artifact(&self, storage_path: &str) -> anyhow::Result<()> {
        let relative_path = safe_relative_path(storage_path)?;
        let path = self.spool_dir.join(relative_path);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err)
                .with_context(|| format!("failed to remove artifact file {}", path.display())),
        }
    }

    pub async fn ensure_spool_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&*self.spool_dir).await.with_context(|| {
            format!(
                "failed to create artifact spool {}",
                self.spool_dir.display()
            )
        })
    }
}

pub fn sanitize_filename(input: &str) -> String {
    let basename = Path::new(input)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let sanitized = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "artifact.bin".to_string()
    } else {
        sanitized
    }
}

fn safe_relative_path(path: &str) -> anyhow::Result<&Path> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        bail!("artifact storage path must be relative and stay under spool root");
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_keeps_safe_ascii() {
        assert_eq!(sanitize_filename("plate-1_file.3mf"), "plate-1_file.3mf");
        assert_eq!(sanitize_filename("../bad name.3mf"), "bad_name.3mf");
        assert_eq!(sanitize_filename("///"), "artifact.bin");
        assert_eq!(sanitize_filename(""), "artifact.bin");
        assert_eq!(sanitize_filename("@@@"), "___");
    }

    #[tokio::test]
    async fn write_artifact_rejects_empty_and_oversized_payloads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = JobStorageConfig::new(temp_dir.path(), 4).unwrap();

        assert!(
            storage
                .write_artifact(TenantId::new(), "artifact", "plate.3mf", b"")
                .await
                .is_err()
        );
        assert!(
            storage
                .write_artifact(TenantId::new(), "artifact", "plate.3mf", b"12345")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn write_and_remove_artifact_use_relative_storage_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = JobStorageConfig::new(temp_dir.path(), 1024).unwrap();
        let tenant_id = TenantId::new();

        let artifact = storage
            .write_artifact(tenant_id, "artifact", "plate file.3mf", b"abc")
            .await
            .unwrap();

        assert_eq!(artifact.filename, "plate_file.3mf");
        assert_eq!(artifact.size_bytes, 3);
        assert!(
            temp_dir
                .path()
                .join(&artifact.storage_path)
                .try_exists()
                .unwrap()
        );

        storage
            .remove_artifact(&artifact.storage_path)
            .await
            .unwrap();
        assert!(
            !temp_dir
                .path()
                .join(&artifact.storage_path)
                .try_exists()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn remove_artifact_rejects_unsafe_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = JobStorageConfig::new(temp_dir.path(), 1024).unwrap();

        assert!(storage.remove_artifact("../escape").await.is_err());
        assert!(storage.remove_artifact("/tmp/escape").await.is_err());
    }
}
