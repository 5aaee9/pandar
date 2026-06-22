use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};

use crate::{AppState, routes::ApiError};

pub(crate) fn authorize_bootstrap(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    let Some(configured_token) = state.bootstrap_token() else {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "bootstrap_disabled"));
    };
    if token != configured_token {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    }

    Ok(())
}
