use std::{collections::BTreeMap, time::Duration};

use axum::{
    extract::{
        FromRequestParts,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Request},
    response::Response,
};
mod send;

use crate::{
    AppState,
    printer_events::{PrinterEventEpochGate, PrinterProjectionChange, ProjectionSubscription},
    routes::{ApiError, auth},
};
use send::{StudioPrinterEventFrame, send_studio_frame, send_studio_message};

const STUDIO_PROJECTION_VERSION: u32 = 1;
const STUDIO_PING_INTERVAL: Duration = Duration::from_secs(20);
const STUDIO_PONG_GRACE: Duration = Duration::from_secs(10);

type PublishedRecords = BTreeMap<String, String>;

pub(super) async fn printer_events_studio(
    state: AppState,
    tenant_id: String,
    version: Option<String>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
) -> Result<Response, ApiError> {
    let tenant_id = super::super::parse_tenant_id(&tenant_id)?;
    if version.as_deref() != Some("1") {
        return Err(ApiError::bad_request("unsupported_printer_event_version"));
    }
    if !state.no_auth_enabled() {
        let authenticated =
            auth::authorize_plugin_studio_for_tenant(&state, &headers, tenant_id).await?;
        debug_assert_eq!(authenticated.token.tenant_id, tenant_id);
    }
    let changes = state
        .printer_events()
        .subscribe_projection_changes(tenant_id)
        .await;
    let epoch = state.printer_events().subscribe_epoch(tenant_id);
    let epoch_gate = state.printer_events().epoch_gate();
    let subscription = state.printer_events().track_subscription(tenant_id).await;
    let (mut parts, _) = request.into_parts();
    let upgrade = WebSocketUpgrade::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| ApiError::bad_request("websocket_upgrade_required"))?;

    Ok(upgrade.on_upgrade(move |socket| {
        forward_studio_events(
            socket,
            state,
            tenant_id,
            changes,
            epoch,
            epoch_gate,
            subscription,
        )
    }))
}

async fn forward_studio_events(
    mut socket: WebSocket,
    state: AppState,
    tenant_id: pandar_core::TenantId,
    mut changes: ProjectionSubscription,
    mut epoch: crate::printer_events::PrinterEventEpoch,
    epoch_gate: PrinterEventEpochGate,
    _subscription: crate::metrics::SubscriptionGuard,
) {
    let devices =
        match super::super::plugin::studio_devices::plugin_printer_devices(&state, tenant_id).await
        {
            Ok(devices) => devices,
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    "failed to build studio printer snapshot"
                );
                return;
            }
        };
    if !send_studio_frame(
        &mut socket,
        &StudioPrinterEventFrame::SnapshotBegin {
            version: STUDIO_PROJECTION_VERSION,
        },
        &mut epoch,
        &epoch_gate,
    )
    .await
    {
        return;
    }

    let mut published = PublishedRecords::new();
    for device in devices {
        let printer_id = device.pandar_printer_id().to_owned();
        let fingerprint = match serde_json::to_string(&device) {
            Ok(fingerprint) => fingerprint,
            Err(err) => {
                tracing::error!(
                    error = %format!("{err:#}"),
                    "failed to fingerprint studio printer snapshot record"
                );
                return;
            }
        };
        if !send_studio_frame(
            &mut socket,
            &StudioPrinterEventFrame::PrinterUpsert {
                printer: Box::new(device),
            },
            &mut epoch,
            &epoch_gate,
        )
        .await
        {
            return;
        }
        published.insert(printer_id, fingerprint);
    }

    let publication = changes.lock_publication().await;
    let buffered = match changes.drain_buffered() {
        Ok(buffered) => buffered,
        Err(skipped) => {
            tracing::error!(
                skipped,
                "studio printer event websocket receiver lagged before snapshot commit"
            );
            return;
        }
    };
    if !send_studio_frame(
        &mut socket,
        &StudioPrinterEventFrame::SnapshotEnd,
        &mut epoch,
        &epoch_gate,
    )
    .await
    {
        return;
    }
    drop(publication);

    for change in buffered {
        if !resolve_and_send_projection_change(
            &state,
            tenant_id,
            &change,
            &mut published,
            &mut socket,
            &mut epoch,
            &epoch_gate,
        )
        .await
        {
            return;
        }
    }

    let start = tokio::time::Instant::now() + STUDIO_PING_INTERVAL;
    let mut ping_interval = tokio::time::interval_at(start, STUDIO_PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut pong_deadline: Option<tokio::time::Instant> = None;
    loop {
        let pong_timeout = async {
            match pong_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
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
            received = changes.recv() => match received {
                Ok(change) => {
                    if !resolve_and_send_projection_change(
                        &state,
                        tenant_id,
                        &change,
                        &mut published,
                        &mut socket,
                        &mut epoch,
                        &epoch_gate,
                    )
                    .await
                    {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::error!(skipped, "studio printer event websocket receiver lagged");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            inbound = socket.recv() => match inbound {
                Some(Ok(Message::Ping(payload))) => {
                    if let Err(err) = socket.send(Message::Pong(payload)).await {
                        tracing::error!(
                            error = %format!("{err:#}"),
                            "failed to send studio printer event websocket pong"
                        );
                        break;
                    }
                }
                Some(Ok(Message::Pong(_))) => {
                    pong_deadline = None;
                }
                Some(Ok(Message::Text(_) | Message::Binary(_))) => {}
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(err)) => {
                    tracing::error!(
                        error = %format!("{err:#}"),
                        "failed to read studio printer event websocket message"
                    );
                    break;
                }
            },
            _ = ping_interval.tick() => {
                if !send_studio_message(
                    &mut socket,
                    Message::Ping(Default::default()),
                    &mut epoch,
                    &epoch_gate,
                )
                .await
                {
                    break;
                }
                pong_deadline = Some(tokio::time::Instant::now() + STUDIO_PONG_GRACE);
            }
            _ = pong_timeout => {
                tracing::warn!("studio printer event websocket ping unanswered; closing websocket");
                break;
            }
        }
    }
}

async fn resolve_and_send_projection_change(
    state: &AppState,
    tenant_id: pandar_core::TenantId,
    change: &PrinterProjectionChange,
    published: &mut PublishedRecords,
    socket: &mut WebSocket,
    epoch: &mut crate::printer_events::PrinterEventEpoch,
    epoch_gate: &PrinterEventEpochGate,
) -> bool {
    let record = match super::super::plugin::studio_devices::studio_projection_record(
        state, tenant_id, change,
    )
    .await
    {
        Ok(record) => record,
        Err(err) => {
            tracing::error!(
                error = ?err,
                "failed to resolve studio projection change"
            );
            return false;
        }
    };
    match record {
        super::super::plugin::studio_devices::StudioProjectionRecord::Upsert(printer) => {
            let fingerprint = match serde_json::to_string(&printer) {
                Ok(fingerprint) => fingerprint,
                Err(err) => {
                    tracing::error!(
                        error = %format!("{err:#}"),
                        "failed to fingerprint studio projection change"
                    );
                    return false;
                }
            };
            if published.get(&change.printer_id) == Some(&fingerprint) {
                return true;
            }
            if !send_studio_frame(
                socket,
                &StudioPrinterEventFrame::PrinterUpsert { printer },
                epoch,
                epoch_gate,
            )
            .await
            {
                return false;
            }
            published.insert(change.printer_id.clone(), fingerprint);
            true
        }
        super::super::plugin::studio_devices::StudioProjectionRecord::Removed {
            dev_id,
            pandar_printer_id,
        } => {
            if !published.contains_key(&pandar_printer_id) {
                return true;
            }
            if !send_studio_frame(
                socket,
                &StudioPrinterEventFrame::PrinterRemoved {
                    dev_id,
                    pandar_printer_id: pandar_printer_id.clone(),
                },
                epoch,
                epoch_gate,
            )
            .await
            {
                return false;
            }
            published.remove(&pandar_printer_id);
            true
        }
    }
}
