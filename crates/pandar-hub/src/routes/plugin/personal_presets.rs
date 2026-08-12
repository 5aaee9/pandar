use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::{
        AuthenticatedTenantToken, CreatePersonalPreset, PersonalPreset, PersonalPresetMetadata,
        PersonalPresetType, RepositoryError,
    },
    routes::{ApiError, auth},
};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct PresetListQuery {
    bundle_version: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct PresetRequest {
    #[serde(rename = "type")]
    preset_type: PersonalPresetType,
    name: String,
    version: String,
    #[serde(default)]
    base_id: String,
    #[serde(default)]
    inherits: Option<String>,
    #[serde(default)]
    filament_id: Option<String>,
    #[serde(default)]
    options: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct PresetMetadataResponse {
    setting_id: String,
    #[serde(rename = "type")]
    preset_type: PersonalPresetType,
    name: String,
    version: String,
    base_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inherits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filament_id: Option<String>,
    updated_time: i64,
}

#[derive(Debug, Serialize)]
struct PresetResponse {
    setting_id: String,
    #[serde(rename = "type")]
    preset_type: PersonalPresetType,
    name: String,
    version: String,
    base_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inherits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filament_id: Option<String>,
    options: BTreeMap<String, String>,
    updated_time: i64,
}

#[derive(Debug, Serialize)]
struct PresetListResponse {
    message: &'static str,
    presets: Vec<PresetMetadataResponse>,
}

#[derive(Debug, Serialize)]
struct PresetMutationResponse {
    message: &'static str,
    setting_id: String,
    updated_time: i64,
}

#[derive(Debug)]
pub(in crate::routes) struct PersonalPresetApiError {
    status: StatusCode,
    error: &'static str,
    code: Option<u8>,
}

#[derive(Debug, Serialize)]
struct PresetErrorResponse {
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<u8>,
}

pub(in crate::routes) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<PresetListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, PersonalPresetApiError> {
    let authenticated = authorize(&state, &headers).await?;
    let Query(query) = query.map_err(|_| invalid_preset())?;
    if !valid_version(&query.bundle_version) {
        return Err(invalid_preset());
    }
    let owner = authenticated
        .session_user
        .as_ref()
        .expect("authorized user")
        .id
        .clone();
    let presets = state
        .personal_presets()
        .list_metadata(authenticated.token.tenant_id, &owner)
        .await?
        .into_iter()
        .map(PresetMetadataResponse::from)
        .collect();
    Ok(no_store(Json(PresetListResponse {
        message: "success",
        presets,
    })))
}

pub(in crate::routes) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(setting_id): Path<String>,
) -> Result<Response, PersonalPresetApiError> {
    let authenticated = authorize(&state, &headers).await?;
    let owner = authenticated
        .session_user
        .as_ref()
        .expect("authorized user")
        .id
        .clone();
    let preset = state
        .personal_presets()
        .get(authenticated.token.tenant_id, &owner, &setting_id)
        .await?
        .ok_or_else(not_found)?;
    Ok(no_store(Json(PresetResponse::from(preset))))
}

pub(in crate::routes) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<PresetRequest>, JsonRejection>,
) -> Result<Response, PersonalPresetApiError> {
    let authenticated = authorize(&state, &headers).await?;
    let Json(payload) = payload.map_err(json_rejection)?;
    let owner = authenticated
        .session_user
        .as_ref()
        .expect("authorized user")
        .id
        .clone();
    let preset = state
        .personal_presets()
        .create_with_audit(
            authenticated.token.tenant_id,
            &owner,
            payload.into(),
            auth::plugin_audit_actor(&authenticated),
        )
        .await?;
    Ok(no_store((
        StatusCode::CREATED,
        Json(PresetMutationResponse::from(preset)),
    )))
}

pub(in crate::routes) async fn replace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(setting_id): Path<String>,
    payload: Result<Json<PresetRequest>, JsonRejection>,
) -> Result<Response, PersonalPresetApiError> {
    let authenticated = authorize(&state, &headers).await?;
    let Json(payload) = payload.map_err(json_rejection)?;
    let owner = authenticated
        .session_user
        .as_ref()
        .expect("authorized user")
        .id
        .clone();
    let preset = state
        .personal_presets()
        .replace_with_audit(
            authenticated.token.tenant_id,
            &owner,
            &setting_id,
            payload.into(),
            auth::plugin_audit_actor(&authenticated),
        )
        .await?;
    Ok(no_store(Json(PresetMutationResponse::from(preset))))
}

pub(in crate::routes) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(setting_id): Path<String>,
) -> Result<Response, PersonalPresetApiError> {
    let authenticated = authorize(&state, &headers).await?;
    let owner = authenticated
        .session_user
        .as_ref()
        .expect("authorized user")
        .id
        .clone();
    state
        .personal_presets()
        .delete_with_audit(
            authenticated.token.tenant_id,
            &owner,
            &setting_id,
            auth::plugin_audit_actor(&authenticated),
        )
        .await?;
    Ok(no_store(StatusCode::NO_CONTENT))
}

async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedTenantToken, PersonalPresetApiError> {
    auth::authorize_plugin_studio_user(state, headers)
        .await
        .map_err(Into::into)
}

fn valid_version(version: &str) -> bool {
    let mut count = 0;
    for part in version.split('.') {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        count += 1;
    }
    count >= 3
}

fn no_store(response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn invalid_preset() -> PersonalPresetApiError {
    PersonalPresetApiError {
        status: StatusCode::BAD_REQUEST,
        error: "invalid_personal_preset",
        code: None,
    }
}

fn json_rejection(rejection: JsonRejection) -> PersonalPresetApiError {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        PersonalPresetApiError {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            error: "personal_preset_too_large",
            code: None,
        }
    } else {
        invalid_preset()
    }
}

fn not_found() -> PersonalPresetApiError {
    PersonalPresetApiError {
        status: StatusCode::NOT_FOUND,
        error: "personal_preset_not_found",
        code: None,
    }
}

impl From<PresetRequest> for CreatePersonalPreset {
    fn from(value: PresetRequest) -> Self {
        Self {
            preset_type: value.preset_type,
            name: value.name,
            version: value.version,
            base_id: value.base_id,
            inherits: value.inherits,
            filament_id: value.filament_id,
            options: value.options,
        }
    }
}

impl From<PersonalPresetMetadata> for PresetMetadataResponse {
    fn from(value: PersonalPresetMetadata) -> Self {
        Self {
            setting_id: value.id,
            preset_type: value.preset_type,
            name: value.name,
            version: value.version,
            base_id: value.base_id,
            inherits: value.inherits,
            filament_id: value.filament_id,
            updated_time: value.updated_time,
        }
    }
}

impl From<PersonalPreset> for PresetResponse {
    fn from(value: PersonalPreset) -> Self {
        Self {
            setting_id: value.id,
            preset_type: value.preset_type,
            name: value.name,
            version: value.version,
            base_id: value.base_id,
            inherits: value.inherits,
            filament_id: value.filament_id,
            options: value.options,
            updated_time: value.updated_time,
        }
    }
}

impl From<PersonalPreset> for PresetMutationResponse {
    fn from(value: PersonalPreset) -> Self {
        Self {
            message: "success",
            setting_id: value.id,
            updated_time: value.updated_time,
        }
    }
}

impl From<ApiError> for PersonalPresetApiError {
    fn from(value: ApiError) -> Self {
        Self {
            status: value.status,
            error: value.code,
            code: None,
        }
    }
}

impl From<RepositoryError> for PersonalPresetApiError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::PersonalPresetLimitExceeded => Self {
                status: StatusCode::CONFLICT,
                error: "personal_preset_limit_exceeded",
                code: Some(14),
            },
            other => ApiError::from(other).into(),
        }
    }
}

impl IntoResponse for PersonalPresetApiError {
    fn into_response(self) -> Response {
        no_store((
            self.status,
            Json(PresetErrorResponse {
                error: self.error,
                code: self.code,
            }),
        ))
    }
}
