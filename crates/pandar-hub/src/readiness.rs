use serde::Serialize;
use std::net::SocketAddr;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub checks: ReadinessChecks,
}

#[derive(Debug, Serialize)]
pub struct ReadinessChecks {
    pub database: ReadinessCheck,
    pub grpc: ReadinessCheck,
    pub spool: ReadinessCheck,
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
    let spool = match state.job_storage().ensure_spool_dir().await {
        Ok(()) => ReadinessCheck::ready("ok"),
        Err(err) => {
            tracing::warn!(error = %crate::redaction::redact_secrets(&format!("{err:#}")), "readiness spool check failed");
            ReadinessCheck::not_ready("unavailable")
        }
    };
    let external_auth = external_auth_check(state).await;

    state.metrics().set_readyz("database", database.ready).await;
    state.metrics().set_readyz("grpc", grpc.ready).await;
    state.metrics().set_readyz("spool", spool.ready).await;
    state
        .metrics()
        .set_readyz("external_auth", external_auth.ready)
        .await;

    let ready = database.ready && grpc.ready && spool.ready && external_auth.ready;
    ReadinessResponse {
        status: if ready { "ready" } else { "not_ready" },
        checks: ReadinessChecks {
            database,
            grpc,
            spool,
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
