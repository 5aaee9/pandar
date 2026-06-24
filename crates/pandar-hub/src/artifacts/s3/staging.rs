use std::path::PathBuf;

use anyhow::{Context, bail};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::artifacts::{ArtifactBody, ArtifactUploadBody};

#[derive(Debug)]
pub(super) struct StagedS3Upload {
    path: PathBuf,
}

impl StagedS3Upload {
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(super) async fn cleanup(self) {
        let _ = fs::remove_file(self.path).await;
    }
}

pub(super) async fn staged_upload_body(
    body: ArtifactUploadBody<'_>,
    max_artifact_bytes: usize,
    key: &str,
) -> anyhow::Result<(StagedS3Upload, u64)> {
    let path = temp_artifact_path("s3-staged-upload");
    staged_upload_body_with_path(body, max_artifact_bytes, key, path, false, false).await
}

#[cfg(test)]
pub(super) async fn staged_upload_body_with_path(
    body: ArtifactUploadBody<'_>,
    max_artifact_bytes: usize,
    key: &str,
    path: PathBuf,
    fail_before_flush: bool,
    fail_before_reopen: bool,
) -> anyhow::Result<(StagedS3Upload, u64)> {
    staged_upload_body_at_path(
        body,
        max_artifact_bytes,
        key,
        path,
        fail_before_flush,
        fail_before_reopen,
    )
    .await
}

#[cfg(not(test))]
async fn staged_upload_body_with_path(
    body: ArtifactUploadBody<'_>,
    max_artifact_bytes: usize,
    key: &str,
    path: PathBuf,
    fail_before_flush: bool,
    fail_before_reopen: bool,
) -> anyhow::Result<(StagedS3Upload, u64)> {
    staged_upload_body_at_path(
        body,
        max_artifact_bytes,
        key,
        path,
        fail_before_flush,
        fail_before_reopen,
    )
    .await
}

async fn staged_upload_body_at_path(
    body: ArtifactUploadBody<'_>,
    max_artifact_bytes: usize,
    key: &str,
    path: PathBuf,
    fail_before_flush: bool,
    fail_before_reopen: bool,
) -> anyhow::Result<(StagedS3Upload, u64)> {
    let mut reader = body.reader;
    let mut file = fs::File::create(&path)
        .await
        .context("failed to create staged S3 artifact upload")?;
    let mut size_bytes = 0usize;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(read) => read,
            Err(err) => {
                cleanup_staged_file(file, &path).await;
                return Err(err).context("failed to read staged artifact upload");
            }
        };
        if read == 0 {
            break;
        }
        size_bytes = size_bytes.saturating_add(read);
        if size_bytes > max_artifact_bytes {
            cleanup_staged_file(file, &path).await;
            bail!("artifact is larger than configured maximum of {max_artifact_bytes} bytes");
        }
        if let Err(err) = file.write_all(&buffer[..read]).await {
            cleanup_staged_file(file, &path).await;
            return Err(err).with_context(|| format!("failed to stage S3 artifact object {key}"));
        }
    }
    if size_bytes == 0 {
        cleanup_staged_file(file, &path).await;
        bail!("artifact bytes cannot be empty");
    }
    if fail_before_flush {
        cleanup_staged_file(file, &path).await;
        bail!("injected staged S3 artifact flush failure");
    }
    if let Err(err) = file.sync_all().await {
        cleanup_staged_file(file, &path).await;
        return Err(err).context("failed to flush staged S3 artifact upload");
    }
    drop(file);
    if fail_before_reopen {
        let _ = fs::remove_file(&path).await;
    }
    if let Err(err) = fs::File::open(&path).await {
        let _ = fs::remove_file(&path).await;
        return Err(err).context("failed to reopen staged S3 artifact upload");
    }
    Ok((StagedS3Upload { path }, size_bytes as u64))
}

pub(super) async fn byte_stream_to_artifact_body(
    mut body: aws_sdk_s3::primitives::ByteStream,
) -> anyhow::Result<ArtifactBody> {
    let path = temp_artifact_path("s3-object-body");
    let mut file = fs::File::create(&path)
        .await
        .context("failed to create staged S3 object response body")?;
    while let Some(bytes) = match body.try_next().await {
        Ok(bytes) => bytes,
        Err(err) => {
            drop(file);
            let _ = fs::remove_file(&path).await;
            return Err(err).context("failed to read S3 response body");
        }
    } {
        if let Err(err) = file.write_all(&bytes).await {
            drop(file);
            let _ = fs::remove_file(&path).await;
            return Err(err).context("failed to stage S3 object response body");
        }
    }
    if let Err(err) = file.sync_all().await {
        drop(file);
        let _ = fs::remove_file(&path).await;
        return Err(err).context("failed to flush staged S3 object response body");
    }
    drop(file);
    let body = fs::File::open(&path)
        .await
        .context("failed to open staged S3 object response body")?;
    let _ = fs::remove_file(path).await;
    Ok(body)
}

async fn cleanup_staged_file(file: fs::File, path: &std::path::Path) {
    drop(file);
    let _ = fs::remove_file(path).await;
}

pub(super) fn temp_artifact_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
}
