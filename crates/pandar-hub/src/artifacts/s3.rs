use std::{path::Path, sync::Arc};

use anyhow::{Context, bail};
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    Client,
    config::SharedCredentialsProvider,
    error::SdkError,
    operation::{delete_object::DeleteObjectError, get_object::GetObjectError},
    primitives::ByteStream,
};
#[cfg(test)]
use tokio::fs;

use super::{
    ArtifactBody, ArtifactStorage, ArtifactStorageBackend, StoreArtifactInput, StoredArtifact,
    sanitize_filename, validate_max_artifact_bytes,
};

mod redaction;
mod staging;

use redaction::redacted_s3_error;
#[cfg(test)]
use staging::temp_artifact_path;
use staging::{byte_stream_to_artifact_body, staged_upload_body};

#[derive(Debug, Clone)]
pub struct S3ArtifactStorageConfig {
    bucket: String,
    region: String,
    endpoint: Option<String>,
    access_key_id: String,
    secret_access_key: String,
    force_path_style: bool,
    max_artifact_bytes: usize,
}

impl S3ArtifactStorageConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_env_values(
            env_value("PANDAR_ARTIFACT_S3_BUCKET")?,
            env_value("PANDAR_ARTIFACT_S3_REGION")?,
            env_value("PANDAR_ARTIFACT_S3_ENDPOINT")?,
            env_value("PANDAR_ARTIFACT_S3_ACCESS_KEY_ID")?,
            env_value("PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY")?,
            env_value("PANDAR_ARTIFACT_S3_FORCE_PATH_STYLE")?,
            env_value("PANDAR_MAX_ARTIFACT_BYTES")?,
        )
    }

    pub fn from_env_values(
        bucket: Option<impl AsRef<str>>,
        region: Option<impl AsRef<str>>,
        endpoint: Option<impl AsRef<str>>,
        access_key_id: Option<impl AsRef<str>>,
        secret_access_key: Option<impl AsRef<str>>,
        force_path_style: Option<impl AsRef<str>>,
        max_artifact_bytes: Option<impl AsRef<str>>,
    ) -> anyhow::Result<Self> {
        let bucket = required_env("PANDAR_ARTIFACT_S3_BUCKET", bucket)?;
        let region = required_env("PANDAR_ARTIFACT_S3_REGION", region)?;
        let endpoint = optional_trimmed(endpoint);
        let access_key_id = required_env("PANDAR_ARTIFACT_S3_ACCESS_KEY_ID", access_key_id)?;
        let secret_access_key =
            required_env("PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY", secret_access_key)?;
        let force_path_style = match force_path_style.as_ref().map(|value| value.as_ref().trim()) {
            None | Some("") => false,
            Some("true") => true,
            Some("false") => false,
            Some(_) => bail!("PANDAR_ARTIFACT_S3_FORCE_PATH_STYLE must be true or false"),
        };
        let max_artifact_bytes = match max_artifact_bytes {
            Some(value) => value
                .as_ref()
                .parse::<usize>()
                .context("failed to parse PANDAR_MAX_ARTIFACT_BYTES")?,
            None => super::DEFAULT_MAX_ARTIFACT_BYTES,
        };
        validate_max_artifact_bytes(max_artifact_bytes)?;

        Ok(Self {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            force_path_style,
            max_artifact_bytes,
        })
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    pub fn force_path_style(&self) -> bool {
        self.force_path_style
    }

    pub fn max_artifact_bytes(&self) -> usize {
        self.max_artifact_bytes
    }

    pub async fn build(&self) -> anyhow::Result<S3ArtifactStorage> {
        let credentials = Credentials::new(
            self.access_key_id.clone(),
            self.secret_access_key.clone(),
            None,
            None,
            "pandar-artifact-s3",
        );
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .load()
            .await;
        let mut builder =
            aws_sdk_s3::config::Builder::from(&sdk_config).force_path_style(self.force_path_style);
        if let Some(endpoint) = &self.endpoint {
            builder = builder.endpoint_url(endpoint);
        }

        Ok(S3ArtifactStorage::new(
            self.bucket.clone(),
            self.max_artifact_bytes,
            Arc::new(Client::from_conf(builder.build())),
        ))
    }
}

pub struct S3ArtifactStorage {
    bucket: String,
    max_artifact_bytes: usize,
    client: Arc<dyn S3ObjectClient>,
}

impl S3ArtifactStorage {
    fn new(
        bucket: impl Into<String>,
        max_artifact_bytes: usize,
        client: Arc<dyn S3ObjectClient>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            max_artifact_bytes,
            client,
        }
    }

    #[cfg(test)]
    fn new_for_test(
        bucket: impl Into<String>,
        max_artifact_bytes: usize,
        client: Arc<dyn S3ObjectClient>,
    ) -> Self {
        Self::new(bucket, max_artifact_bytes, client)
    }
}

#[async_trait::async_trait]
impl ArtifactStorage for S3ArtifactStorage {
    async fn put_artifact(&self, input: StoreArtifactInput<'_>) -> anyhow::Result<StoredArtifact> {
        let filename = sanitize_filename(input.filename);
        let key = s3_object_key(input.tenant_id, input.artifact_id)?;
        let (staged, size_bytes) =
            staged_upload_body(input.body, self.max_artifact_bytes, &key).await?;
        let uploaded = self
            .client
            .put_object(&self.bucket, &key, staged.path())
            .await
            .context("failed to write artifact to S3 object");
        staged.cleanup().await;
        uploaded?;

        Ok(StoredArtifact {
            filename,
            storage_key: key.clone(),
            storage_path: key,
            size_bytes,
            backend: ArtifactStorageBackend::S3,
        })
    }

    async fn open_artifact(&self, storage_key: &str) -> anyhow::Result<ArtifactBody> {
        self.client
            .get_object(&self.bucket, storage_key)
            .await
            .context("failed to read artifact from S3 object")
    }

    async fn delete_artifact(&self, storage_key: &str) -> anyhow::Result<()> {
        match self.client.delete_object(&self.bucket, storage_key).await {
            Ok(()) => Ok(()),
            Err(err) if delete_error_is_not_found(&err) => Ok(()),
            Err(err) => Err(err).context("failed to delete artifact S3 object"),
        }
    }

    async fn check_ready(&self) -> anyhow::Result<()> {
        self.client
            .check_bucket(&self.bucket)
            .await
            .context("artifact S3 bucket is not reachable")
    }

    fn max_artifact_bytes(&self) -> usize {
        self.max_artifact_bytes
    }

    fn backend(&self) -> ArtifactStorageBackend {
        ArtifactStorageBackend::S3
    }

    fn is_not_found(&self, err: &anyhow::Error) -> bool {
        open_error_is_not_found(err)
    }
}

#[async_trait::async_trait]
trait S3ObjectClient: Send + Sync {
    async fn put_object(&self, bucket: &str, key: &str, body_path: &Path) -> anyhow::Result<()>;
    async fn get_object(&self, bucket: &str, key: &str) -> anyhow::Result<ArtifactBody>;
    async fn delete_object(&self, bucket: &str, key: &str) -> anyhow::Result<()>;
    async fn check_bucket(&self, bucket: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl S3ObjectClient for Client {
    async fn put_object(&self, bucket: &str, key: &str, body_path: &Path) -> anyhow::Result<()> {
        let body = ByteStream::from_path(body_path)
            .await
            .context("failed to open staged artifact for S3 upload")?;
        self.put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .send()
            .await
            .map_err(redacted_s3_error("put object"))?;
        Ok(())
    }

    async fn get_object(&self, bucket: &str, key: &str) -> anyhow::Result<ArtifactBody> {
        let output = self
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|err| {
                if sdk_get_error_is_not_found(&err) {
                    S3ObjectNotFound.into()
                } else {
                    redacted_s3_error("get object")(err)
                }
            })?;
        byte_stream_to_artifact_body(output.body).await
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> anyhow::Result<()> {
        match self.delete_object().bucket(bucket).key(key).send().await {
            Ok(_) => Ok(()),
            Err(err) if sdk_delete_error_is_not_found(&err) => Err(S3ObjectNotFound.into()),
            Err(err) => Err(redacted_s3_error("delete object")(err)),
        }
    }

    async fn check_bucket(&self, bucket: &str) -> anyhow::Result<()> {
        self.head_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(redacted_s3_error("head bucket"))?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("S3 object not found")]
struct S3ObjectNotFound;

#[cfg(test)]
impl S3ObjectNotFound {
    fn new() -> Self {
        Self
    }
}

#[cfg(test)]
async fn bytes_to_artifact_body(bytes: Vec<u8>) -> anyhow::Result<ArtifactBody> {
    let path = temp_artifact_path("s3-object-body");
    fs::write(&path, bytes)
        .await
        .context("failed to stage S3 object response body")?;
    let body = fs::File::open(&path)
        .await
        .context("failed to open staged S3 object response body")?;
    let _ = fs::remove_file(path).await;
    Ok(body)
}

fn s3_object_key(tenant_id: pandar_core::TenantId, artifact_id: &str) -> anyhow::Result<String> {
    if artifact_id.is_empty() || artifact_id.contains('/') || artifact_id.contains("..") {
        bail!("artifact id cannot be used as S3 object key");
    }
    Ok(format!("{tenant_id}/{artifact_id}"))
}

fn env_value(name: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err).with_context(|| format!("failed to read {name}")),
    }
}

fn required_env(name: &str, value: Option<impl AsRef<str>>) -> anyhow::Result<String> {
    match value.as_ref().map(|value| value.as_ref().trim()) {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        _ => bail!("{name} is required when PANDAR_ARTIFACT_STORAGE=s3"),
    }
}

fn optional_trimmed(value: Option<impl AsRef<str>>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.as_ref().trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn delete_error_is_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<S3ObjectNotFound>().is_some()
}

pub(super) fn open_error_is_not_found(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.downcast_ref::<S3ObjectNotFound>().is_some())
}

fn sdk_delete_error_is_not_found(err: &SdkError<DeleteObjectError>) -> bool {
    if err
        .as_service_error()
        .is_some_and(delete_service_error_is_not_found)
    {
        return true;
    }

    err.raw_response()
        .is_some_and(|response| response.status().as_u16() == 404)
}

fn sdk_get_error_is_not_found(err: &SdkError<GetObjectError>) -> bool {
    if err
        .as_service_error()
        .is_some_and(get_service_error_is_not_found)
    {
        return true;
    }

    err.raw_response()
        .is_some_and(|response| response.status().as_u16() == 404)
}

fn get_service_error_is_not_found(err: &GetObjectError) -> bool {
    aws_smithy_types::error::metadata::ProvideErrorMetadata::code(err)
        .is_some_and(|code| code == "NoSuchKey" || code == "NotFound")
}

fn delete_service_error_is_not_found(err: &DeleteObjectError) -> bool {
    aws_smithy_types::error::metadata::ProvideErrorMetadata::code(err)
        .is_some_and(|code| code == "NoSuchKey" || code == "NotFound")
}

#[cfg(test)]
mod tests;
