use axum::{http::header, response::IntoResponse};

const HUB_CLIENT_CONTRACT: &str = include_str!("../../../../contracts/hub-client.openapi.json");

pub(super) async fn hub_client_contract() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        HUB_CLIENT_CONTRACT,
    )
}
