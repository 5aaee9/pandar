use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::value::RawValue;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        Error as WebSocketError, Message,
        client::IntoClientRequest,
        http::{HeaderValue, header},
    },
};
use tokio_util::sync::CancellationToken;

use super::{Fence, StreamConfig, StreamSignals, cache, notify_dispatcher};
use crate::{
    connection::{ConnectionState, DispatcherWake},
    studio_status::{PrinterObservation, project_stream_device},
};

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) enum StreamOutcome {
    AuthRejected,
    Failed { committed: bool },
    Fenced,
    Cancelled,
}

pub(super) async fn dial_and_stream(
    state: &Arc<Mutex<ConnectionState>>,
    signals: &Arc<StreamSignals>,
    dispatcher: &Arc<Mutex<Option<DispatcherWake>>>,
    config: &StreamConfig,
    cancel: &CancellationToken,
) -> StreamOutcome {
    let request = match build_request(config) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("pandar printer event stream request build failed: {error:#}");
            return StreamOutcome::Failed { committed: false };
        }
    };
    let connect =
        tokio::time::timeout(DIAL_TIMEOUT, tokio_tungstenite::connect_async(request)).await;
    let (socket, _) = match connect {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            let auth_rejected = matches!(
                &error,
                WebSocketError::Http(response)
                    if matches!(response.status().as_u16(), 401 | 403)
            );
            if auth_rejected {
                eprintln!("pandar printer event stream authentication rejected: {error:#}");
                cache::apply_auth_rejected(state, Fence::of(config));
                notify_dispatcher(dispatcher);
                return StreamOutcome::AuthRejected;
            }
            eprintln!("pandar printer event stream dial failed: {error:#}");
            return StreamOutcome::Failed { committed: false };
        }
        Err(error) => {
            eprintln!("pandar printer event stream dial timed out: {error:#}");
            return StreamOutcome::Failed { committed: false };
        }
    };
    if !Fence::of(config).matches(&state.lock().expect("connection state")) {
        return StreamOutcome::Fenced;
    }
    protocol_loop(
        socket,
        Fence::of(config),
        state,
        signals,
        dispatcher,
        cancel,
    )
    .await
}

fn build_request(
    config: &StreamConfig,
) -> anyhow::Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let mut request = config
        .url
        .clone()
        .into_client_request()
        .context("build Hub printer event WebSocket request")?;
    if !config.token.trim().is_empty() {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.token))
                .context("encode Hub bearer token")?,
        );
    }
    Ok(request)
}

enum Phase {
    AwaitingBegin,
    Staging(Vec<PrinterObservation>),
    Live,
}

async fn protocol_loop(
    mut socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    fence: Fence,
    state: &Arc<Mutex<ConnectionState>>,
    signals: &Arc<StreamSignals>,
    dispatcher: &Arc<Mutex<Option<DispatcherWake>>>,
    cancel: &CancellationToken,
) -> StreamOutcome {
    let mut phase = Phase::AwaitingBegin;
    let mut committed = false;
    loop {
        let received = tokio::select! {
            _ = cancel.cancelled() => return StreamOutcome::Cancelled,
            received = tokio::time::timeout(READ_IDLE_TIMEOUT, socket.next()) => match received {
                Ok(Some(Ok(message))) => message,
                Ok(Some(Err(error))) => {
                    eprintln!("pandar printer event stream read failed: {error:#}");
                    return StreamOutcome::Failed { committed };
                }
                Ok(None) => {
                    eprintln!("pandar printer event stream closed by Hub");
                    return StreamOutcome::Failed { committed };
                }
                Err(error) => {
                    eprintln!("pandar printer event stream idle beyond keepalive bound: {error:#}");
                    return StreamOutcome::Failed { committed };
                }
            },
        };
        match received {
            Message::Ping(_) => {
                if let Err(error) = socket.flush().await {
                    eprintln!("pandar printer event stream Pong write failed: {error:#}");
                    return StreamOutcome::Failed { committed };
                }
            }
            Message::Pong(_) => {}
            Message::Text(text) => {
                phase = match handle_text(&text, phase, &mut committed, fence, state, signals) {
                    TextFlow::Continue(next) => {
                        notify_dispatcher(dispatcher);
                        next
                    }
                    TextFlow::ProtocolFailure(error) => {
                        eprintln!("pandar printer event stream protocol failure: {error:#}");
                        return StreamOutcome::Failed { committed };
                    }
                    TextFlow::Fenced => return StreamOutcome::Fenced,
                };
            }
            Message::Close(frame) => {
                eprintln!("pandar printer event stream closed: {frame:?}");
                return StreamOutcome::Failed { committed };
            }
            other => {
                eprintln!("pandar printer event stream unexpected frame: {other:?}");
                return StreamOutcome::Failed { committed };
            }
        }
    }
}

enum TextFlow {
    Continue(Phase),
    ProtocolFailure(anyhow::Error),
    Fenced,
}

fn protocol_failure(message: &'static str) -> TextFlow {
    TextFlow::ProtocolFailure(anyhow::anyhow!(message))
}

fn handle_text(
    text: &str,
    phase: Phase,
    committed: &mut bool,
    fence: Fence,
    state: &Arc<Mutex<ConnectionState>>,
    signals: &Arc<StreamSignals>,
) -> TextFlow {
    let frame = match decode_frame(text) {
        Ok(frame) => frame,
        Err(error) => {
            return TextFlow::ProtocolFailure(error.context("decode printer event stream frame"));
        }
    };
    match frame {
        StreamFrame::SnapshotBegin { version } => {
            if version != 1 {
                return protocol_failure("unsupported printer event version");
            }
            match phase {
                Phase::AwaitingBegin => TextFlow::Continue(Phase::Staging(Vec::new())),
                _ => protocol_failure("unexpected snapshot_begin"),
            }
        }
        StreamFrame::PrinterUpsert { printer } => {
            let observation = match project_stream_device(printer.get()) {
                Ok(observation) => observation,
                Err(error) => {
                    return TextFlow::ProtocolFailure(
                        error.context("project streamed Hub printer record"),
                    );
                }
            };
            match phase {
                Phase::Staging(mut staged) => {
                    if staged
                        .iter()
                        .any(|current| current.dev_id == observation.dev_id)
                    {
                        return protocol_failure("duplicate printer in initial snapshot");
                    }
                    staged.push(observation);
                    TextFlow::Continue(Phase::Staging(staged))
                }
                Phase::Live => {
                    if cache::apply_upsert(state, fence, observation) {
                        TextFlow::Continue(Phase::Live)
                    } else {
                        TextFlow::Fenced
                    }
                }
                Phase::AwaitingBegin => protocol_failure("upsert before snapshot_begin"),
            }
        }
        StreamFrame::SnapshotEnd => match phase {
            Phase::Staging(staged) => {
                if cache::apply_snapshot(state, signals, fence, staged) {
                    *committed = true;
                    TextFlow::Continue(Phase::Live)
                } else {
                    TextFlow::Fenced
                }
            }
            _ => protocol_failure("snapshot_end without staged snapshot"),
        },
        StreamFrame::PrinterRemoved {
            dev_id,
            pandar_printer_id,
        } => match phase {
            Phase::Live => {
                if cache::apply_removal(state, fence, &dev_id, &pandar_printer_id) {
                    TextFlow::Continue(Phase::Live)
                } else {
                    TextFlow::Fenced
                }
            }
            _ => protocol_failure("removal before snapshot_end"),
        },
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamFrameKind {
    SnapshotBegin,
    PrinterUpsert,
    SnapshotEnd,
    PrinterRemoved,
}

#[derive(Deserialize)]
struct BeginFrame {
    version: u32,
}

#[derive(Deserialize)]
struct UpsertFrame {
    printer: Box<RawValue>,
}

#[derive(Deserialize)]
struct RemovedFrame {
    dev_id: String,
    pandar_printer_id: String,
}

fn decode_frame(text: &str) -> anyhow::Result<StreamFrame> {
    let kind: StreamFrameKind = serde_json::from_str(text).map_err(anyhow::Error::msg)?;
    Ok(match kind {
        StreamFrameKind::SnapshotBegin => StreamFrame::SnapshotBegin {
            version: serde_json::from_str::<BeginFrame>(text)?.version,
        },
        StreamFrameKind::PrinterUpsert => StreamFrame::PrinterUpsert {
            printer: serde_json::from_str::<UpsertFrame>(text)?.printer,
        },
        StreamFrameKind::SnapshotEnd => StreamFrame::SnapshotEnd,
        StreamFrameKind::PrinterRemoved => {
            let removed = serde_json::from_str::<RemovedFrame>(text)?;
            StreamFrame::PrinterRemoved {
                dev_id: removed.dev_id,
                pandar_printer_id: removed.pandar_printer_id,
            }
        }
    })
}

enum StreamFrame {
    SnapshotBegin {
        version: u32,
    },
    PrinterUpsert {
        printer: Box<RawValue>,
    },
    SnapshotEnd,
    PrinterRemoved {
        dev_id: String,
        pandar_printer_id: String,
    },
}
