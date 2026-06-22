use axum::{
    Json,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    AppState,
    bootstrap::authorize_bootstrap,
    repositories::RecordAuditEvent,
    routes::{ApiError, HubSummary, TenantListResponse, TenantResponse},
};

#[derive(Debug, Deserialize)]
pub(super) struct CreateTenantRequest {
    slug: String,
    display_name: String,
}

pub(super) async fn summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<HubSummary>, ApiError> {
    authorize_bootstrap(&state, &headers)?;

    Ok(Json(HubSummary {
        tenants: state.tenants().count().await?,
        agents: state.agents().count().await?,
        printers: state.printers().count().await?,
        commands: state.commands().count().await?,
    }))
}

pub(super) async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TenantListResponse>, ApiError> {
    authorize_bootstrap(&state, &headers)?;

    let tenants = state
        .tenants()
        .list()
        .await?
        .into_iter()
        .map(TenantResponse::from)
        .collect();

    Ok(Json(TenantListResponse { tenants }))
}

pub(super) async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateTenantRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TenantResponse>), ApiError> {
    authorize_bootstrap(&state, &headers)?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.slug.trim().is_empty() || payload.display_name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let tenant = state
        .tenants()
        .create(payload.slug, payload.display_name)
        .await?;
    state
        .audit_events()
        .record(RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "tenant.create".to_owned(),
            target_type: "tenant".to_owned(),
            target_id: Some(tenant.id.to_string()),
            metadata_json: json!({ "tenant_slug": tenant.slug }).to_string(),
        })
        .await?;

    Ok((StatusCode::CREATED, Json(TenantResponse::from(tenant))))
}
