use std::{
    convert::Infallible,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use axum::extract::ws::Message;
use futures_util::Sink;
use tokio::sync::oneshot;

use super::{LinearizedSendOutcome, linearized_send};
use crate::printer_events::PrinterEventHub;

struct ControlledSink {
    frames: Arc<Mutex<Vec<Message>>>,
    enqueued: Option<oneshot::Sender<()>>,
    invalidate_on_ready: Option<PrinterEventHub>,
    block_flush: bool,
}

impl ControlledSink {
    fn new(block_flush: bool) -> (Self, Arc<Mutex<Vec<Message>>>) {
        let frames = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                frames: frames.clone(),
                enqueued: None,
                invalidate_on_ready: None,
                block_flush,
            },
            frames,
        )
    }
}

impl Sink<Message> for ControlledSink {
    type Error = Infallible;

    fn poll_ready(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if let Some(hub) = self.invalidate_on_ready.take() {
            hub.invalidate_epoch();
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        self.frames.lock().unwrap().push(item);
        if let Some(enqueued) = self.enqueued.take() {
            let _ = enqueued.send(());
        }
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if self.block_flush {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_close(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(context)
    }
}

#[tokio::test]
async fn invalidation_before_linearized_enqueue_records_no_frame() {
    let hub = PrinterEventHub::new();
    let mut epoch = hub.subscribe_epoch();
    let gate = hub.epoch_gate();
    let (mut sink, frames) = ControlledSink::new(false);
    sink.invalidate_on_ready = Some(hub.clone());

    let outcome = linearized_send(&mut sink, Message::Text("stale".into()), &mut epoch, &gate)
        .await
        .unwrap();

    assert!(matches!(outcome, LinearizedSendOutcome::EpochChanged));
    assert!(frames.lock().unwrap().is_empty());
}

#[tokio::test]
async fn enqueue_before_invalidation_is_ordered_but_blocked_flush_is_cancelled() {
    let hub = PrinterEventHub::new();
    let mut epoch = hub.subscribe_epoch();
    let gate = hub.epoch_gate();
    let (enqueued_sender, enqueued_receiver) = oneshot::channel();
    let (mut sink, frames) = ControlledSink::new(true);
    sink.enqueued = Some(enqueued_sender);
    let send = tokio::spawn(async move {
        linearized_send(
            &mut sink,
            Message::Text("ordered-before-boundary".into()),
            &mut epoch,
            &gate,
        )
        .await
        .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(1), enqueued_receiver)
        .await
        .expect("frame should reach start_send")
        .expect("enqueue signal should stay open");
    assert_eq!(frames.lock().unwrap().len(), 1);

    hub.invalidate_epoch();

    let outcome = tokio::time::timeout(Duration::from_secs(1), send)
        .await
        .expect("epoch change should cancel a blocked flush")
        .unwrap();
    assert!(matches!(outcome, LinearizedSendOutcome::EpochChanged));
    assert_eq!(frames.lock().unwrap().len(), 1);
}
