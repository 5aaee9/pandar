pub use crate::artifacts::{
    DEFAULT_MAX_ARTIFACT_BYTES, FilesystemArtifactStorage as JobStorageConfig, StoredArtifact,
    sanitize_filename,
};

#[cfg(test)]
mod tests {
    use super::*;
    use pandar_core::TenantId;

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
