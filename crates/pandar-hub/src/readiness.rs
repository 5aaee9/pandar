use serde::Serialize;
use std::net::SocketAddr;

use crate::{
    AppState, artifacts::ArtifactStorageBackend, cluster::ControlPlaneKind, db::DatabaseBackend,
};

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub checks: ReadinessChecks,
}

#[derive(Debug, Serialize)]
pub struct ReadinessChecks {
    pub database: ReadinessCheck,
    pub grpc: ReadinessCheck,
    pub artifact_storage: ReadinessCheck,
    pub external_auth: ReadinessCheck,
}

#[derive(Debug, Serialize)]
pub struct ReadinessCheck {
    pub ready: bool,
    pub detail: String,
}

pub async fn check(state: &AppState) -> ReadinessResponse {
    let database = match state.tenants().count().await {
        Ok(_) => ReadinessCheck::ready("ok"),
        Err(err) => {
            tracing::warn!(error = %format!("{err:#}"), "readiness database check failed");
            ReadinessCheck::not_ready("unavailable")
        }
    };
    let grpc = grpc_check();
    let artifact_storage = artifact_storage_check(state).await;
    let external_auth = external_auth_check(state).await;

    state.metrics().set_readyz("database", database.ready).await;
    state.metrics().set_readyz("grpc", grpc.ready).await;
    state
        .metrics()
        .set_readyz("artifact_storage", artifact_storage.ready)
        .await;
    state
        .metrics()
        .set_readyz("external_auth", external_auth.ready)
        .await;

    let ready = database.ready && grpc.ready && artifact_storage.ready && external_auth.ready;
    ReadinessResponse {
        status: if ready { "ready" } else { "not_ready" },
        checks: ReadinessChecks {
            database,
            grpc,
            artifact_storage,
            external_auth,
        },
    }
}

impl ReadinessCheck {
    fn ready(detail: impl Into<String>) -> Self {
        Self {
            ready: true,
            detail: detail.into(),
        }
    }

    fn not_ready(detail: impl Into<String>) -> Self {
        Self {
            ready: false,
            detail: detail.into(),
        }
    }
}

fn grpc_check() -> ReadinessCheck {
    match std::env::var("PANDAR_HUB_GRPC_BIND") {
        Ok(value) if value.parse::<SocketAddr>().is_ok() => ReadinessCheck::ready("configured"),
        Ok(value) => {
            tracing::warn!(grpc_bind = %value, "readiness gRPC config check failed");
            ReadinessCheck::not_ready("invalid_configuration")
        }
        Err(std::env::VarError::NotPresent) => ReadinessCheck::ready("configured"),
        Err(err) => {
            tracing::warn!(error = %format!("{err:#}"), "readiness gRPC config check failed");
            ReadinessCheck::not_ready("invalid_configuration")
        }
    }
}

async fn external_auth_check(state: &AppState) -> ReadinessCheck {
    let Some(verifier) = state.external_auth() else {
        return ReadinessCheck::ready("disabled");
    };

    match verifier.check_ready().await {
        Ok(()) => ReadinessCheck::ready("configured"),
        Err(err) => {
            tracing::warn!(error = %format!("{err:#}"), "readiness external auth check failed");
            ReadinessCheck::not_ready("invalid_configuration")
        }
    }
}

async fn artifact_storage_check(state: &AppState) -> ReadinessCheck {
    if !filesystem_artifact_storage_shared_ready(
        state.database_backend(),
        state.control_plane().kind(),
        state.artifact_storage().backend(),
        filesystem_shared_override(),
    ) {
        tracing::warn!(
            "readiness artifact storage check failed: PostgreSQL plus NATS requires shared filesystem override or object storage"
        );
        return ReadinessCheck::not_ready("filesystem_not_shared");
    }

    match state.artifact_storage().check_ready().await {
        Ok(()) => ReadinessCheck::ready("ok"),
        Err(err) => {
            tracing::warn!(error = %crate::redaction::redact_secrets(&format!("{err:#}")), "readiness artifact storage check failed");
            ReadinessCheck::not_ready("unavailable")
        }
    }
}

fn filesystem_shared_override() -> bool {
    std::env::var("PANDAR_ARTIFACT_FILESYSTEM_SHARED")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

fn filesystem_artifact_storage_shared_ready(
    database_backend: DatabaseBackend,
    control_plane: ControlPlaneKind,
    artifact_storage: ArtifactStorageBackend,
    filesystem_shared: bool,
) -> bool {
    !matches!(
        (
            database_backend,
            control_plane,
            artifact_storage,
            filesystem_shared,
        ),
        (
            DatabaseBackend::Postgres,
            ControlPlaneKind::Nats,
            ArtifactStorageBackend::Filesystem,
            false,
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_nats_filesystem_requires_shared_override() {
        assert!(!filesystem_artifact_storage_shared_ready(
            DatabaseBackend::Postgres,
            ControlPlaneKind::Nats,
            ArtifactStorageBackend::Filesystem,
            false,
        ));
        assert!(filesystem_artifact_storage_shared_ready(
            DatabaseBackend::Postgres,
            ControlPlaneKind::Nats,
            ArtifactStorageBackend::Filesystem,
            true,
        ));
        assert!(filesystem_artifact_storage_shared_ready(
            DatabaseBackend::Postgres,
            ControlPlaneKind::Nats,
            ArtifactStorageBackend::S3,
            false,
        ));
        assert!(filesystem_artifact_storage_shared_ready(
            DatabaseBackend::Sqlite,
            ControlPlaneKind::InProcess,
            ArtifactStorageBackend::Filesystem,
            false,
        ));
    }
}
