use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::routes) struct RetiredApiTokenError {
    error: &'static str,
}

pub(in crate::routes) async fn list_api_tokens() -> (StatusCode, Json<RetiredApiTokenError>) {
    retired_api_tokens()
}

pub(in crate::routes) async fn create_api_token() -> (StatusCode, Json<RetiredApiTokenError>) {
    retired_api_tokens()
}

pub(in crate::routes) async fn revoke_api_token() -> (StatusCode, Json<RetiredApiTokenError>) {
    retired_api_tokens()
}

fn retired_api_tokens() -> (StatusCode, Json<RetiredApiTokenError>) {
    (
        StatusCode::GONE,
        Json(RetiredApiTokenError {
            error: "api_tokens_retired",
        }),
    )
}
