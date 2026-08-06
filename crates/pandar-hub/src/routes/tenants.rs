use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::RecordAuditEvent,
    routes::{ApiError, TenantResponse, auth, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub(super) struct UpdateTenantRequest {
    display_name: String,
}

#[derive(Debug, Serialize)]
struct TenantRenameAuditMetadata<'a> {
    previous_display_name: &'a str,
    display_name: &'a str,
}

pub(super) async fn update_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<UpdateTenantRequest>, JsonRejection>,
) -> Result<Json<TenantResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let principal =
        auth::authorize_tenant_admin_user_or_no_auth(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.display_name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let (tenant, previous_display_name) = state
        .tenants()
        .update_display_name(tenant_id, payload.display_name)
        .await?;
    let actor = auth::audit_actor(&principal);
    state
        .audit_events()
        .record(RecordAuditEvent {
            tenant_id,
            actor_type: actor.actor_type,
            user_id: actor.user_id,
            action: "tenant.rename".to_owned(),
            target_type: "tenant".to_owned(),
            target_id: Some(tenant_id.to_string()),
            metadata_json: serde_json::to_string(&TenantRenameAuditMetadata {
                previous_display_name: &previous_display_name,
                display_name: &tenant.display_name,
            })
            .expect("tenant rename audit metadata is serializable"),
        })
        .await?;

    Ok(Json(TenantResponse::from(tenant)))
}
