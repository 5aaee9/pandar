use std::sync::{Arc, Mutex};

use crate::artifacts::{
    ArtifactStorage, ArtifactStorageBackend, StoreArtifactInput, StoredArtifact,
};

#[derive(Clone, Default)]
pub(super) struct RecordingArtifactStorage {
    deleted: Arc<Mutex<Vec<String>>>,
    fail_delete: bool,
}

impl RecordingArtifactStorage {
    pub(super) fn failing() -> Self {
        Self {
            deleted: Arc::new(Mutex::new(Vec::new())),
            fail_delete: true,
        }
    }

    pub(super) fn deleted(&self) -> Vec<String> {
        self.deleted.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ArtifactStorage for RecordingArtifactStorage {
    async fn put_artifact(&self, _input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact> {
        unimplemented!("cleanup tests only delete artifacts")
    }

    async fn open_artifact(
        &self,
        _storage_key: &str,
    ) -> anyhow::Result<crate::artifacts::ArtifactBody> {
        unimplemented!("cleanup tests only delete artifacts")
    }

    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()> {
        self.deleted.lock().unwrap().push(storage_key.to_string());
        if self.fail_delete {
            anyhow::bail!("delete failed");
        }
        Ok(())
    }

    async fn check_ready(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn max_artifact_bytes(&self) -> usize {
        1024
    }

    fn backend(&self) -> ArtifactStorageBackend {
        ArtifactStorageBackend::Filesystem
    }
}
