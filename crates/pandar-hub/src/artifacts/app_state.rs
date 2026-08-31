use std::sync::Arc;

use super::ArtifactStorage;

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
