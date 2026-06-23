use axum::{
    Json,
    extract::{
        FromRequestParts, Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    response::Response,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    metrics::TicketMetric,
    repositories::{PrinterEventTicketConsumeResult, UserRole, generate_secret, hash_secret},
    routes::{ApiError, auth},
};

#[derive(Debug, Deserialize)]
pub(super) struct PrinterEventQuery {
    ticket: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct PrinterEventTicketResponse {
    ticket: String,
    expires_at: String,
}

pub(super) async fn printer_events(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<PrinterEventQuery>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
) -> Result<Response, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    if headers.contains_key(AUTHORIZATION) {
        auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    } else if let Some(ticket) = query.ticket {
        match state
            .printer_event_tickets()
            .consume(tenant_id, &hash_secret(&ticket))
            .await?
        {
            PrinterEventTicketConsumeResult::Consumed(_) => {
                state.metrics().record_ticket(TicketMetric::Consumed)
            }
            PrinterEventTicketConsumeResult::Expired => {
                state.metrics().record_ticket(TicketMetric::Expired);
                return Err(ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "invalid_auth_token",
                ));
            }
            PrinterEventTicketConsumeResult::Invalid => {
                state.metrics().record_ticket(TicketMetric::Invalid);
                return Err(ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "invalid_auth_token",
                ));
            }
        }
    } else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    }
    state.printers().list_for_tenant(tenant_id).await?;
    let subscription = state.printer_events().track_subscription(tenant_id).await;
    let receiver = state.printer_events().subscribe(tenant_id).await;
    let (mut parts, _) = request.into_parts();
    let upgrade = WebSocketUpgrade::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| ApiError::bad_request("websocket_upgrade_required"))?;

    Ok(upgrade.on_upgrade(move |socket| forward_events(socket, receiver, subscription)))
}

pub(super) async fn create_printer_event_ticket(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PrinterEventTicketResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    state.printers().list_for_tenant(tenant_id).await?;
    let ticket = generate_secret("pandar_ws");
    let issued = state
        .printer_event_tickets()
        .issue(tenant_id, hash_secret(&ticket))
        .await?;
    state.metrics().record_ticket(TicketMetric::Issued);

    Ok(Json(PrinterEventTicketResponse {
        ticket,
        expires_at: issued.expires_at,
    }))
}

async fn forward_events(
    mut socket: WebSocket,
    mut receiver: tokio::sync::broadcast::Receiver<crate::printer_events::PrinterEvent>,
    _subscription: crate::metrics::SubscriptionGuard,
) {
    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::error!(skipped, "printer event websocket receiver lagged");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        let text = match serde_json::to_string(&event) {
            Ok(text) => text,
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "failed to encode printer event");
                break;
            }
        };
        if let Err(err) = socket.send(Message::Text(text.into())).await {
            tracing::error!(error = %format!("{err:#}"), "failed to send printer event websocket message");
            break;
        }
    }
}
