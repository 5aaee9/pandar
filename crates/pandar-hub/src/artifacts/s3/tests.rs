use super::*;
use crate::artifacts::{ArtifactUploadBody, s3::staging::staged_upload_body_with_path};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use std::{
    pin::Pin,
    task::{Context as TaskContext, Poll},
};
use tokio::io::{AsyncReadExt, ReadBuf};

#[test]
fn missing_bucket_fails_with_env_var_name() {
    let err = S3ArtifactStorageConfig::from_env_values(
        None::<&str>,
        Some("us-east-1"),
        None::<&str>,
        Some("access"),
        Some("secret"),
        None::<&str>,
        Some("1024"),
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_ARTIFACT_S3_BUCKET"));
}

#[test]
fn missing_credentials_fail_with_relevant_env_var_name() {
    let missing_access_key = S3ArtifactStorageConfig::from_env_values(
        Some("bucket"),
        Some("us-east-1"),
        None::<&str>,
        None::<&str>,
        Some("secret"),
        None::<&str>,
        Some("1024"),
    )
    .unwrap_err();
    assert!(format!("{missing_access_key:#}").contains("PANDAR_ARTIFACT_S3_ACCESS_KEY_ID"));

    let missing_secret_key = S3ArtifactStorageConfig::from_env_values(
        Some("bucket"),
        Some("us-east-1"),
        None::<&str>,
        Some("access"),
        None::<&str>,
        None::<&str>,
        Some("1024"),
    )
    .unwrap_err();
    assert!(format!("{missing_secret_key:#}").contains("PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY"));
}

#[test]
fn custom_endpoint_and_force_path_style_parse_into_config() {
    let config = S3ArtifactStorageConfig::from_env_values(
        Some("bucket"),
        Some("us-east-1"),
        Some("http://localhost:9000"),
        Some("access"),
        Some("secret"),
        Some("true"),
        Some("1024"),
    )
    .unwrap();

    assert_eq!(config.bucket(), "bucket");
    assert_eq!(config.region(), "us-east-1");
    assert_eq!(config.endpoint(), Some("http://localhost:9000"));
    assert!(config.force_path_style());
    assert_eq!(config.max_artifact_bytes(), 1024);
}

#[tokio::test]
async fn object_keys_are_tenant_artifact_scoped_without_filename_authority() {
    let client = Arc::new(FakeS3ObjectClient::default());
    let storage = S3ArtifactStorage::new_for_test("bucket", 1024, client.clone());
    let tenant_id = pandar_core::TenantId::new();

    let artifact = storage
        .put_artifact(StoreArtifactInput {
            tenant_id,
            artifact_id: "artifact",
            filename: "../browser/name.3mf",
            body: ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
        })
        .await
        .unwrap();

    assert_eq!(artifact.filename, "name.3mf");
    assert_eq!(artifact.storage_key, format!("{tenant_id}/artifact"));
    let calls = client.calls.lock().unwrap();
    assert_eq!(
        calls[0],
        ("put", "bucket".to_string(), artifact.storage_key)
    );
}

#[tokio::test]
async fn s3_storage_round_trips_and_delete_ignores_not_found() {
    let client = Arc::new(FakeS3ObjectClient::default());
    let storage = S3ArtifactStorage::new_for_test("bucket", 1024, client.clone());

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
    body.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"abc");

    storage
        .delete_artifact(&artifact.storage_key)
        .await
        .unwrap();
    storage
        .delete_artifact(&artifact.storage_key)
        .await
        .unwrap();
}

#[tokio::test]
async fn check_ready_uses_bucket_reachability() {
    let client = Arc::new(FakeS3ObjectClient::default());
    let storage = S3ArtifactStorage::new_for_test("bucket", 1024, client.clone());

    storage.check_ready().await.unwrap();
    client
        .readiness_error
        .lock()
        .unwrap()
        .replace("blocked".into());
    let err = storage.check_ready().await.unwrap_err();

    assert!(format!("{err:#}").contains("artifact S3 bucket is not reachable"));
}

#[test]
fn redacted_s3_error_preserves_safe_context_without_credentials() {
    let err = redacted_s3_error("put object")(
        "status=403 code=SignatureDoesNotMatch message=access key PANDARACCESS123 secret PANDARSECRET456 failed",
    );
    let formatted = format!("{err:#}");

    assert!(formatted.contains("S3 put object request failed"));
    assert!(formatted.contains("status=403"));
    assert!(formatted.contains("code=SignatureDoesNotMatch"));
    assert!(formatted.contains("message=access key [redacted] secret [redacted] failed"));
    assert!(!formatted.contains("PANDARACCESS123"));
    assert!(!formatted.contains("PANDARSECRET456"));
}

#[test]
fn redacted_s3_error_redacts_quoted_access_key_id_context() {
    let err = redacted_s3_error("head bucket")(
        r#"status=403 code=InvalidAccessKeyId message=credential rejected access_key_id: "PANDARACCESS123""#,
    );
    let formatted = format!("{err:#}");

    assert!(formatted.contains("S3 head bucket request failed"));
    assert!(formatted.contains("status=403"));
    assert!(formatted.contains("code=InvalidAccessKeyId"));
    assert!(formatted.contains("message=credential rejected"));
    assert!(formatted.contains(r#"access_key_id: "[redacted]""#));
    assert!(!formatted.contains("PANDARACCESS123"));
}

#[test]
fn redacted_s3_error_redacts_quoted_secret_access_key_context() {
    let err = redacted_s3_error("put object")(
        r#"status=403 code=SignatureDoesNotMatch message=credential rejected secret_access_key: \"PANDARSECRET456\""#,
    );
    let formatted = format!("{err:#}");

    assert!(formatted.contains("S3 put object request failed"));
    assert!(formatted.contains("status=403"));
    assert!(formatted.contains("code=SignatureDoesNotMatch"));
    assert!(formatted.contains("message=credential rejected"));
    assert!(formatted.contains(r#"secret_access_key: \"[redacted]\""#));
    assert!(!formatted.contains("PANDARSECRET456"));
}

#[test]
fn redacted_s3_error_redacts_aws_xml_access_key_context() {
    let err = redacted_s3_error("put object")(
        "<Error><Code>InvalidAccessKeyId</Code><Message>bad key</Message><AWSAccessKeyId>PANDARACCESS123</AWSAccessKeyId></Error>",
    );
    let formatted = format!("{err:#}");

    assert!(formatted.contains("S3 put object request failed"));
    assert!(formatted.contains("InvalidAccessKeyId"));
    assert!(formatted.contains("bad key"));
    assert!(formatted.contains("<AWSAccessKeyId>[redacted]</AWSAccessKeyId>"));
    assert!(!formatted.contains("PANDARACCESS123"));
}

#[tokio::test]
async fn staged_upload_removes_temp_file_when_reopen_fails() {
    let temp_root = tempfile::tempdir().unwrap();
    let path = temp_root.path().join("staged-upload");

    let err = staged_upload_body_with_path(
        ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
        1024,
        "tenant/artifact",
        path.clone(),
        false,
        true,
    )
    .await
    .unwrap_err();

    assert!(format!("{err:#}").contains("failed to reopen staged S3 artifact upload"));
    assert!(!path.try_exists().unwrap());
}

#[tokio::test]
async fn staged_upload_removes_temp_file_when_reader_fails_after_write() {
    let temp_root = tempfile::tempdir().unwrap();
    let path = temp_root.path().join("staged-upload");

    let err = staged_upload_body_with_path(
        ArtifactUploadBody::reader(FailingAfterPrefixReader {
            emitted_prefix: false,
        }),
        1024,
        "tenant/artifact",
        path.clone(),
        false,
        false,
    )
    .await
    .unwrap_err();

    assert!(format!("{err:#}").contains("upload stream failed"));
    assert!(!path.try_exists().unwrap());
}

#[tokio::test]
async fn staged_upload_removes_temp_file_when_flush_fails() {
    let temp_root = tempfile::tempdir().unwrap();
    let path = temp_root.path().join("staged-upload");

    let err = staged_upload_body_with_path(
        ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
        1024,
        "tenant/artifact",
        path.clone(),
        true,
        false,
    )
    .await
    .unwrap_err();

    assert!(format!("{err:#}").contains("injected staged S3 artifact flush failure"));
    assert!(!path.try_exists().unwrap());
}

#[tokio::test]
async fn staged_upload_keeps_file_until_explicit_cleanup() {
    let temp_root = tempfile::tempdir().unwrap();
    let path = temp_root.path().join("staged-upload");

    let (staged, size_bytes) = staged_upload_body_with_path(
        ArtifactUploadBody::reader(std::io::Cursor::new(b"abc".to_vec())),
        1024,
        "tenant/artifact",
        path.clone(),
        false,
        false,
    )
    .await
    .unwrap();

    assert_eq!(size_bytes, 3);
    assert!(path.try_exists().unwrap());

    staged.cleanup().await;

    assert!(!path.try_exists().unwrap());
}

#[test]
fn delete_error_classifies_only_true_not_found() {
    assert!(delete_error_is_not_found(&anyhow::anyhow!(
        S3ObjectNotFound
    )));
    assert!(!delete_error_is_not_found(&anyhow::anyhow!(
        "status=500 code=InternalError"
    )));
}

#[test]
fn sdk_delete_error_classifier_distinguishes_not_found_from_other_failures() {
    let not_found = DeleteObjectError::generic(
        aws_smithy_types::error::ErrorMetadata::builder()
            .code("NoSuchKey")
            .message("missing")
            .build(),
    );
    let denied = DeleteObjectError::generic(
        aws_smithy_types::error::ErrorMetadata::builder()
            .code("AccessDenied")
            .message("denied")
            .build(),
    );

    assert!(delete_service_error_is_not_found(&not_found));
    assert!(!delete_service_error_is_not_found(&denied));
}

#[test]
fn sdk_get_error_classifier_distinguishes_not_found_from_other_failures() {
    let not_found = GetObjectError::generic(
        aws_smithy_types::error::ErrorMetadata::builder()
            .code("NoSuchKey")
            .message("missing")
            .build(),
    );
    let denied = GetObjectError::generic(
        aws_smithy_types::error::ErrorMetadata::builder()
            .code("AccessDenied")
            .message("denied")
            .build(),
    );

    assert!(get_service_error_is_not_found(&not_found));
    assert!(!get_service_error_is_not_found(&denied));
}

#[derive(Default)]
struct FakeS3ObjectClient {
    objects: Mutex<HashMap<(String, String), Vec<u8>>>,
    calls: Mutex<Vec<(&'static str, String, String)>>,
    readiness_error: Mutex<Option<String>>,
}

struct FailingAfterPrefixReader {
    emitted_prefix: bool,
}

impl tokio::io::AsyncRead for FailingAfterPrefixReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.emitted_prefix {
            return Poll::Ready(Err(std::io::Error::other("upload stream failed")));
        }
        self.emitted_prefix = true;
        buf.put_slice(b"abc");
        Poll::Ready(Ok(()))
    }
}

#[async_trait::async_trait]
impl S3ObjectClient for FakeS3ObjectClient {
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let bytes = tokio::fs::read(body_path).await?;
        self.calls
            .lock()
            .unwrap()
            .push(("put", bucket.to_string(), key.to_string()));
        self.objects
            .lock()
            .unwrap()
            .insert((bucket.to_string(), key.to_string()), bytes);
        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> anyhow::Result<ArtifactBody> {
        self.calls
            .lock()
            .unwrap()
            .push(("get", bucket.to_string(), key.to_string()));
        let bytes = self
            .objects
            .lock()
            .unwrap()
            .get(&(bucket.to_string(), key.to_string()))
            .cloned()
            .ok_or_else(S3ObjectNotFound::new)?;
        bytes_to_artifact_body(bytes).await
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(("delete", bucket.to_string(), key.to_string()));
        self.objects
            .lock()
            .unwrap()
            .remove(&(bucket.to_string(), key.to_string()))
            .ok_or_else(S3ObjectNotFound::new)?;
        Ok(())
    }

    async fn check_bucket(&self, _bucket: &str) -> anyhow::Result<()> {
        if let Some(err) = self.readiness_error.lock().unwrap().clone() {
            anyhow::bail!("{err}");
        }
        Ok(())
    }
}
