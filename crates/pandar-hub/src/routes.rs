#[cfg(test)]
use axum::http::StatusCode;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};
#[cfg(test)]
use pandar_core::TenantId;
use serde::Serialize;

pub(crate) use error::{ApiError, parse_tenant_id};

use crate::AppState;

mod admin;
mod agent_auth;
mod agent_printers;
mod agents;
mod artifacts;
mod audit_events;
mod auth;
mod bootstrap;
mod error;
pub(crate) mod jobs;
mod join_links;
mod onboarding;
mod plugin;
mod printer_events;
pub(crate) mod printer_operations;
mod printers;
mod provisioning;
mod status;
mod tenant_tokens;

pub fn router(state: AppState) -> Router {
    let default_body_limit = 64 * 1024;

    Router::new()
        .route("/healthz", get(status::healthz))
        .route("/api/v1/me", get(onboarding::me))
        .route(
            "/api/v1/onboarding/tenants",
            post(onboarding::create_tenant),
        )
        .route(
            "/api/v1/join-links/accept",
            post(onboarding::accept_join_link),
        )
        .route("/api/v1/summary", get(admin::summary))
        .route(
            "/api/v1/tenants",
            get(admin::list_tenants).post(admin::create_tenant),
        )
        .route(
            "/api/v1/bootstrap/tenant-admin",
            post(bootstrap::create_tenant_admin),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents",
            get(agents::list_agents).post(agents::create_agent),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}",
            delete(agents::delete_agent),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users",
            get(provisioning::list_users).post(provisioning::create_user),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/role",
            axum::routing::patch(provisioning::update_user_role),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/identities",
            get(provisioning::list_user_identities).post(provisioning::link_user_identity),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens",
            get(provisioning::list_api_tokens).post(provisioning::create_api_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/api-tokens/{token_id}",
            axum::routing::delete(provisioning::revoke_api_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/tenant-tokens",
            get(tenant_tokens::list_tenant_tokens).post(tenant_tokens::create_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}",
            axum::routing::delete(tenant_tokens::revoke_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}/rotate",
            post(tenant_tokens::rotate_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/join-links",
            get(join_links::list_join_links).post(join_links::create_join_link),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/join-links/{join_link_id}",
            axum::routing::delete(join_links::revoke_join_link),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/plugin/login-tickets",
            post(plugin::create_login_ticket),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/mobile/login-tickets",
            post(plugin::create_mobile_login_ticket),
        )
        .route(
            "/api/v1/plugin/login-tickets/exchange",
            post(plugin::exchange_login_ticket),
        )
        .route(
            "/api/v1/mobile/login-tickets/exchange",
            post(plugin::exchange_mobile_login_ticket),
        )
        .route(
            "/api/v1/plugin/no-auth-session",
            post(plugin::create_no_auth_session),
        )
        .route("/api/v1/plugin/session", delete(plugin::revoke_session))
        .route("/api/v1/plugin/printers", get(plugin::list_printers))
        .route(
            "/api/v1/plugin/printers/{printer_id}/firmware",
            get(plugin::firmware::get_state),
        )
        .route(
            "/api/v1/plugin/printers/{printer_id}/firmware/refresh",
            post(plugin::firmware::refresh),
        )
        .route(
            "/api/v1/plugin/printers/{printer_id}/firmware/prepare",
            post(plugin::firmware::prepare),
        )
        .route(
            "/api/v1/plugin/printers/{printer_id}/firmware/execute",
            post(plugin::firmware::execute).layer(DefaultBodyLimit::max(
                plugin::firmware::FIRMWARE_EXECUTE_BODY_LIMIT,
            )),
        )
        .route("/api/v1/plugin/jobs", get(plugin::list_jobs))
        .route("/api/v1/plugin/jobs/{id}", get(plugin::get_job))
        .route("/api/v1/plugin/jobs/{id}/plate", get(plugin::get_plate))
        .route(
            "/api/v1/plugin/jobs/{id}/model-task",
            get(plugin::get_model_task),
        )
        .route("/api/v1/plugin/jobs/{id}/subtask", get(plugin::get_subtask))
        .route("/api/v1/plugin/jobs/{id}/cancel", post(plugin::cancel_job))
        .route(
            "/api/v1/plugin/prints",
            post(plugin::create_print).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/v1/plugin/printers/{printer_id}/operations",
            post(plugin::create_printer_operation),
        )
        .route(
            "/api/v1/agents/{agent_id}/artifacts/{artifact_id}",
            get(artifacts::download_agent_artifact),
        )
        .route(
            "/api/v1/agents/{agent_id}/printers",
            get(agent_printers::list_agent_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/audit-events",
            get(audit_events::list_audit_events),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agent-pairings",
            post(provisioning::create_agent_pairing),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate",
            post(provisioning::rotate_agent_credential),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:revoke",
            post(provisioning::revoke_agent_credential),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers",
            get(printers::list_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}",
            get(printers::get_printer)
                .patch(printers::update_printer)
                .delete(printers::delete_printer),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs",
            post(jobs::create_job).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/artifact-metadata-preview",
            post(jobs::preview_artifact_metadata).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls",
            post(printers::printer_control),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}/camera.mp4",
            get(printers::printer_camera_stream),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh",
            post(printers::refresh_printer_materials),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs",
            get(jobs::list_jobs).delete(jobs::clear_jobs),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}",
            get(jobs::get_job).delete(jobs::delete_job),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch",
            post(jobs::retry_dispatch),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint",
            post(jobs::reprint),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate",
            post(jobs::duplicate),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers",
            post(printers::refresh_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers",
            post(printers::discover_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer",
            post(printers::diagnose_printer),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer",
            post(printers::link_printer),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/commands/{command_id}",
            get(printers::get_command),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printer-events",
            get(printer_events::printer_events),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printer-events/tickets",
            post(printer_events::create_printer_event_ticket),
        )
        .layer(DefaultBodyLimit::max(default_body_limit))
        .with_state(state)
}

pub fn observability_router(state: AppState) -> Router {
    Router::new()
        .route("/readyz", get(status::readyz))
        .route("/metrics", get(status::metrics))
        .with_state(state)
}

#[derive(Debug, Serialize)]
pub(super) struct HubSummary {
    tenants: i64,
    agents: i64,
    printers: i64,
    commands: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct TenantResponse {
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct TenantListResponse {
    tenants: Vec<TenantResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct AgentResponse {
    id: String,
    tenant_id: String,
    name: String,
    status: String,
    created_at: String,
}

#[cfg(test)]
mod tests;
