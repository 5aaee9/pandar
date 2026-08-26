use axum::extract::ws::Message;
use futures_util::Sink;
use serde::Serialize;

use super::super::{LinearizedSendOutcome, linearized_send};
use crate::printer_events::PrinterEventEpochGate;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum StudioPrinterEventFrame {
    SnapshotBegin {
        version: u32,
    },
    PrinterUpsert {
        printer: Box<super::super::super::plugin::studio_devices::PluginPrinterResponse>,
    },
    SnapshotEnd,
    PrinterRemoved {
        dev_id: String,
        pandar_printer_id: String,
    },
}

pub(super) async fn send_studio_frame<S>(
    sink: &mut S,
    frame: &StudioPrinterEventFrame,
    epoch: &mut crate::printer_events::PrinterEventEpoch,
    gate: &PrinterEventEpochGate,
) -> bool
where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let text = match serde_json::to_string(frame) {
        Ok(text) => text,
        Err(err) => {
            tracing::error!(
                error = %format!("{err:#}"),
                "failed to encode studio printer event frame"
            );
            return false;
        }
    };
    #[cfg(test)]
    super::super::send_pause::wait_after_serialization().await;
    send_studio_message(sink, Message::Text(text.into()), epoch, gate).await
}

pub(super) async fn send_studio_message<S>(
    sink: &mut S,
    message: Message,
    epoch: &mut crate::printer_events::PrinterEventEpoch,
    gate: &PrinterEventEpochGate,
) -> bool
where
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    match linearized_send(sink, message, epoch, gate).await {
        Ok(LinearizedSendOutcome::Flushed) => true,
        Ok(LinearizedSendOutcome::EpochChanged) => {
            tracing::warn!("printer event epoch changed during studio send; closing websocket");
            false
        }
        Ok(LinearizedSendOutcome::EpochClosed(err)) => {
            tracing::error!(
                error = %format!("{err:#}"),
                "printer event epoch closed during studio send; closing websocket"
            );
            false
        }
        Err(err) => {
            tracing::error!(
                error = %format!("{err:#}"),
                "failed to send studio printer event websocket message"
            );
            false
        }
    }
}
