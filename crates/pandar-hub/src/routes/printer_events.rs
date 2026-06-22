use axum::{
    extract::{
        FromRequestParts, Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Request},
    response::Response,
};

use crate::{
    AppState,
    repositories::UserRole,
    routes::{ApiError, auth},
};

pub(super) async fn printer_events(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
) -> Result<Response, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    state.printers().list_for_tenant(tenant_id).await?;
    let receiver = state.printer_events().subscribe(tenant_id).await;
    let (mut parts, _) = request.into_parts();
    let upgrade = WebSocketUpgrade::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| ApiError::bad_request("websocket_upgrade_required"))?;

    Ok(upgrade.on_upgrade(move |socket| forward_events(socket, receiver)))
}

async fn forward_events(
    mut socket: WebSocket,
    mut receiver: tokio::sync::broadcast::Receiver<crate::printer_events::PrinterEvent>,
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
