use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use pandar_core::JobId;

use crate::{
    AppState,
    routes::{ApiError, auth, parse_tenant_id},
};

pub(in crate::routes) async fn delete_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, job_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    state
        .jobs()
        .delete_clearable_for_tenant_with_audit(
            state.artifact_storage(),
            tenant_id,
            job_id,
            auth::audit_actor(&auth),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
