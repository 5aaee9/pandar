use std::pin::Pin;

use axum::{
    Json,
    extract::{
        FromRequestParts, Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    response::Response,
};
use futures_util::{Sink, future::poll_fn};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    metrics::TicketMetric,
    printer_events::{PrinterEventEpoch, PrinterEventEpochGate},
    repositories::{PrinterEventTicketConsumeResult, UserRole, generate_secret, hash_secret},
    routes::{ApiError, auth},
};

#[cfg(test)]
pub(crate) mod send_pause;
#[cfg(test)]
mod send_tests;
mod studio;

#[derive(Debug, Clone)]
pub(crate) enum LinearizedSendOutcome {
    Flushed,
    EpochChanged,
    EpochClosed(tokio::sync::watch::error::RecvError),
}

pub(crate) async fn linearized_send<S>(
    sink: &mut S,
    message: Message,
    epoch: &mut PrinterEventEpoch,
    gate: &PrinterEventEpochGate,
) -> Result<LinearizedSendOutcome, S::Error>
where
    S: Sink<Message> + Unpin,
{
    {
        let ready = poll_fn(|context| Pin::new(&mut *sink).poll_ready(context));
        tokio::pin!(ready);
        tokio::select! {
            biased;

            changed = epoch.changed() => {
                return Ok(match changed {
                    Ok(()) => LinearizedSendOutcome::EpochChanged,
                    Err(err) => LinearizedSendOutcome::EpochClosed(err),
                });
            }
            ready = &mut ready => ready?,
        }
    }

    {
        let _gate = gate.lock();
        match epoch.has_changed() {
            Ok(true) => return Ok(LinearizedSendOutcome::EpochChanged),
            Ok(false) => {}
            Err(err) => return Ok(LinearizedSendOutcome::EpochClosed(err)),
        }
        Pin::new(&mut *sink).start_send(message)?;
    }

    let flush = async {
        #[cfg(test)]
        send_pause::wait_during_flush().await;
        poll_fn(|context| Pin::new(&mut *sink).poll_flush(context)).await
    };
    tokio::pin!(flush);
    tokio::select! {
        biased;

        changed = epoch.changed() => Ok(match changed {
            Ok(()) => LinearizedSendOutcome::EpochChanged,
            Err(err) => LinearizedSendOutcome::EpochClosed(err),
        }),
        flushed = &mut flush => {
            flushed?;
            Ok(LinearizedSendOutcome::Flushed)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct PrinterEventQuery {
    ticket: Option<String>,
    projection: Option<String>,
    version: Option<String>,
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
    if query.projection.as_deref() == Some("studio") {
        return studio::printer_events_studio(state, tenant_id, query.version, headers, request)
            .await;
    }
    printer_events_default(state, tenant_id, query.ticket, headers, request).await
}

async fn printer_events_default(
    state: AppState,
    tenant_id: String,
    ticket: Option<String>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
) -> Result<Response, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    if state.no_auth_enabled() {
        state.metrics().record_ticket(TicketMetric::Consumed);
    } else if headers.contains_key(AUTHORIZATION) {
        auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    } else if let Some(ticket) = ticket {
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
    let epoch = state.printer_events().subscribe_epoch(tenant_id);
    let epoch_gate = state.printer_events().epoch_gate();
    let (mut parts, _) = request.into_parts();
    let upgrade = WebSocketUpgrade::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| ApiError::bad_request("websocket_upgrade_required"))?;

    Ok(upgrade.on_upgrade(move |socket| {
        forward_events(socket, receiver, epoch, epoch_gate, subscription)
    }))
}

pub(super) async fn create_printer_event_ticket(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<PrinterEventTicketResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    if !state.no_auth_enabled() {
        auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    }
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
    mut epoch: PrinterEventEpoch,
    epoch_gate: PrinterEventEpochGate,
    _subscription: crate::metrics::SubscriptionGuard,
) {
    loop {
        let event = tokio::select! {
            biased;

            changed = epoch.changed() => {
                match changed {
                    Ok(()) => tracing::warn!("printer event epoch changed; closing websocket"),
                    Err(err) => tracing::error!(
                        error = %format!("{err:#}"),
                        "printer event epoch closed; closing websocket"
                    ),
                }
                break;
            }
            received = receiver.recv() => match received {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::error!(skipped, "printer event websocket receiver lagged");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        };
        match epoch.has_changed() {
            Ok(true) => {
                tracing::warn!("printer event epoch changed before send; closing websocket");
                break;
            }
            Ok(false) => {}
            Err(err) => {
                tracing::error!(
                    error = %format!("{err:#}"),
                    "printer event epoch closed before send; closing websocket"
                );
                break;
            }
        }
        let text = match serde_json::to_string(&event) {
            Ok(text) => text,
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "failed to encode printer event");
                break;
            }
        };
        #[cfg(test)]
        send_pause::wait_after_serialization().await;
        match linearized_send(
            &mut socket,
            Message::Text(text.into()),
            &mut epoch,
            &epoch_gate,
        )
        .await
        {
            Ok(LinearizedSendOutcome::Flushed) => {}
            Ok(LinearizedSendOutcome::EpochChanged) => {
                tracing::warn!("printer event epoch changed during send; closing websocket");
                break;
            }
            Ok(LinearizedSendOutcome::EpochClosed(err)) => {
                tracing::error!(
                    error = %format!("{err:#}"),
                    "printer event epoch closed during send; closing websocket"
                );
                break;
            }
            Err(err) => {
                tracing::error!(
                    error = %format!("{err:#}"),
                    "failed to send printer event websocket message"
                );
                break;
            }
        }
    }
}
