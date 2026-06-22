use axum::http::Method;
use serde_json::{Value, json};

use super::*;

mod access;
mod agents;
mod workflow;

async fn admin_tenant() -> (AppState, Router, String, String) {
    let state = bootstrap_state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_id = tenant.id.to_string();
    let admin_token = auth_token_for_role(&state, &tenant_id, admin(), "admin-token").await;
    (state, app, tenant_id, admin_token)
}

fn admin() -> crate::repositories::UserRole {
    crate::repositories::UserRole::TenantAdmin
}
