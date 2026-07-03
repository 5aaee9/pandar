use axum::{
    Json,
    extract::Path,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};

use crate::{
    AppState,
    repositories::UserRole,
    routes::{ApiError, auth},
    sessions::LiveDispatchError,
};

use super::{
    CommandResponse, UpdatePrinterRequest,
    helpers::{
        fail_link_printer_dispatch_after_commit, link_printer_hub_command, parse_printer_id,
    },
};

pub(in crate::routes) async fn update_printer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
    payload: Result<Json<UpdatePrinterRequest>, JsonRejection>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let payload = payload.into_payload()?;

    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };
    let Some(token) = state
        .sessions()
        .current_token(tenant_id, printer.agent_id)
        .await
    else {
        return Err(ApiError::new(StatusCode::CONFLICT, "agent_not_connected"));
    };

    state
        .printers()
        .update_name_with_audit(
            tenant_id,
            printer_id,
            payload
                .name
                .clone()
                .expect("update printer payload should contain a name"),
            auth::audit_actor(&auth),
        )
        .await?;
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            printer.agent_id,
            payload.clone(),
            auth::audit_actor(&auth),
        )
        .await?;
    let hub_command = link_printer_hub_command(command.id, &payload);

    match state
        .sessions()
        .try_dispatch_live_command(tenant_id, printer.agent_id, token, command.id, hub_command)
        .await
    {
        Ok(()) => Ok(Json(CommandResponse::from(command))),
        Err(LiveDispatchError::NotCurrent) => {
            let failed = fail_link_printer_dispatch_after_commit(
                command.id,
                tenant_id,
                printer.agent_id,
                &payload,
                "agent connection closed before printer update completed".to_owned(),
                |command_id, tenant_id, agent_id, error| async move {
                    state
                        .commands()
                        .mark_failed(command_id, tenant_id, agent_id, error)
                        .await
                },
            )
            .await?;
            Ok(Json(CommandResponse::from(failed)))
        }
        Err(LiveDispatchError::ChannelClosed | LiveDispatchError::ChannelFull) => {
            let failed = fail_link_printer_dispatch_after_commit(
                command.id,
                tenant_id,
                printer.agent_id,
                &payload,
                "agent command channel unavailable before printer update completed".to_owned(),
                |command_id, tenant_id, agent_id, error| async move {
                    state
                        .commands()
                        .mark_failed(command_id, tenant_id, agent_id, error)
                        .await
                },
            )
            .await?;
            Ok(Json(CommandResponse::from(failed)))
        }
    }
}
