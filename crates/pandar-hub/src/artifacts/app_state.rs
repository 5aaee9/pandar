use std::sync::Arc;

use super::{ArtifactStorage, ArtifactUploadBody, StoreArtifactInput, StoredArtifact};

pub trait IntoArtifactStorage {
    fn into_artifact_storage(self) -> Arc<dyn ArtifactStorage>;
}

impl<T> IntoArtifactStorage for T
where
    T: ArtifactStorage + 'static,
{
    fn into_artifact_storage(self) -> Arc<dyn ArtifactStorage> {
        Arc::new(self)
    }
}

impl IntoArtifactStorage for Arc<dyn ArtifactStorage> {
    fn into_artifact_storage(self) -> Arc<dyn ArtifactStorage> {
        self
    }
}

pub struct JobStorageAlias<'a> {
    storage: &'a dyn ArtifactStorage,
}

impl<'a> JobStorageAlias<'a> {
    pub fn new(storage: &'a dyn ArtifactStorage) -> Self {
        Self { storage }
    }

    pub fn max_artifact_bytes(&self) -> usize {
        self.storage.max_artifact_bytes()
    }

    pub async fn write_artifact(
        &self,
        tenant_id: pandar_core::TenantId,
        artifact_id: &str,
        filename: &str,
        bytes: &[u8],
    ) -> anyhow::Result<StoredArtifact> {
        self.storage
            .put_artifact(StoreArtifactInput {
                tenant_id,
                artifact_id,
                filename,
                body: ArtifactUploadBody::reader(std::io::Cursor::new(bytes.to_vec())),
            })
            .await
    }

    pub async fn remove_artifact(&self, storage_path: &str) -> anyhow::Result<()> {
        self.storage.delete_artifact(storage_path).await
    }

    pub async fn ensure_spool_dir(&self) -> anyhow::Result<()> {
        self.storage.check_ready().await
    }
}
