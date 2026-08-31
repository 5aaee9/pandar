use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, bail};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use super::{
    ArtifactBody, ArtifactStorage, ArtifactStorageBackend, StoreArtifactInput, StoredArtifact,
    validate_max_artifact_bytes,
};

#[derive(Debug, Clone)]
pub struct FilesystemArtifactStorage {
    spool_dir: Arc<PathBuf>,
    max_artifact_bytes: usize,
}

impl FilesystemArtifactStorage {
    pub fn new(spool_dir: impl Into<PathBuf>, max_artifact_bytes: usize) -> anyhow::Result<Self> {
        validate_max_artifact_bytes(max_artifact_bytes)?;

        Ok(Self {
            spool_dir: Arc::new(spool_dir.into()),
            max_artifact_bytes,
        })
    }
}

#[async_trait::async_trait]
impl ArtifactStorage for FilesystemArtifactStorage {
    async fn put_artifact(&self, input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact> {
        let filename = sanitize_filename(input.filename);
        let storage_key = PathBuf::from(input.tenant_id.to_string())
            .join(input.artifact_id)
            .join(&filename);
        let path = self.spool_dir.join(&storage_key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("failed to create artifact directory {}", parent.display())
            })?;
        }

        let mut reader = input.body.reader;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&path)
            .await
            .with_context(|| format!("failed to create artifact file {}", path.display()))?;
        let mut size_bytes = 0usize;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = match reader.read(&mut buffer).await {
                Ok(read) => read,
                Err(err) => {
                    drop(file);
                    let _ = fs::remove_file(&path).await;
                    return Err(err).context("failed to read staged artifact upload");
                }
            };
            if read == 0 {
                break;
            }
            size_bytes = size_bytes.saturating_add(read);
            if size_bytes > self.max_artifact_bytes {
                drop(file);
                let _ = fs::remove_file(&path).await;
                bail!(
                    "artifact is larger than configured maximum of {} bytes",
                    self.max_artifact_bytes
                );
            }
            file.write_all(&buffer[..read])
                .await
                .with_context(|| format!("failed to write artifact file {}", path.display()))?;
        }

        if size_bytes == 0 {
            drop(file);
            let _ = fs::remove_file(&path).await;
            bail!("artifact bytes cannot be empty");
        }

        let storage_key = storage_key.to_string_lossy().into_owned();
        Ok(StoredArtifact {
            filename,
            storage_key,
            size_bytes: size_bytes as u64,
            backend: ArtifactStorageBackend::Filesystem,
        })
    }

    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody> {
        let relative_path = safe_relative_path(storage_key)?;
        let path = self.spool_dir.join(relative_path);
        fs::File::open(&path)
            .await
            .with_context(|| format!("failed to open artifact file {}", path.display()))
    }

    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()> {
        let relative_path = safe_relative_path(storage_key)?;
        let path = self.spool_dir.join(relative_path);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).context("failed to remove artifact file [redacted]"),
        }
    }

    async fn check_ready(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&*self.spool_dir).await.with_context(|| {
            format!(
                "failed to create artifact spool {}",
                self.spool_dir.display()
            )
        })
    }

    fn max_artifact_bytes(&self) -> usize {
        self.max_artifact_bytes
    }

    fn backend(&self) -> ArtifactStorageBackend {
        ArtifactStorageBackend::Filesystem
    }

    fn is_not_found(&self, err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound)
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
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        bail!("artifact storage key must be relative and stay under storage root");
    }

    Ok(path)
}
