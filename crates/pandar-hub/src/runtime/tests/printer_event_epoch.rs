use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::super::*;
use crate::{
    cluster::{ControlMessageStream, ControlPlane, ControlPlaneBackend, HubControlMessage},
    printer_events::{PrinterEvent, PrinterEventCommand},
};

enum SubscribeStep {
    Failure(anyhow::Error),
    Stream(ControlMessageStream),
}

struct ScriptedControlPlaneBackend {
    steps: Mutex<VecDeque<SubscribeStep>>,
    subscribe_calls: AtomicUsize,
}

impl ScriptedControlPlaneBackend {
    fn new(steps: impl IntoIterator<Item = SubscribeStep>) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
            subscribe_calls: AtomicUsize::new(0),
        }
    }

    fn subscribe_calls(&self) -> usize {
        self.subscribe_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ControlPlaneBackend for ScriptedControlPlaneBackend {
    async fn publish(&self, _message: HubControlMessage) -> anyhow::Result<()> {
        Ok(())
    }

    async fn subscribe(&self) -> anyhow::Result<ControlMessageStream> {
        self.subscribe_calls.fetch_add(1, Ordering::SeqCst);
        let step = self.steps.lock().unwrap().pop_front();
        match step {
            Some(SubscribeStep::Failure(err)) => Err(err),
            Some(SubscribeStep::Stream(stream)) => Ok(stream),
            None => futures_util::future::pending().await,
        }
    }
}

#[tokio::test]
async fn printer_event_publish_failure_invalidates_process_epoch() {
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::failing_for_tests());
    let mut epoch = state.printer_events().subscribe_epoch();

    state
        .publish_printer_event(pandar_core::TenantId::new(), test_event("publish-failed"))
        .await;

    tokio::time::timeout(Duration::from_secs(1), epoch.changed())
        .await
        .expect("publish failure should invalidate epoch")
        .expect("epoch sender should stay open");
}

#[tokio::test]
async fn control_plane_item_error_invalidates_epoch_and_keeps_stream_open() {
    let (sender, stream) = control_stream();
    let backend = Arc::new(ScriptedControlPlaneBackend::new([SubscribeStep::Stream(
        stream,
    )]));
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::for_tests(backend.clone()));
    let tenant = state
        .tenants()
        .create("item-error-acme", "Item Error Acme")
        .await
        .unwrap();
    let mut local = state.printer_events().subscribe(tenant.id).await;
    let mut epoch = state.printer_events().subscribe_epoch();
    let (_task, ready) = spawn_control_plane_ready(state);
    ready.await.unwrap().unwrap();

    sender
        .send(Err(
            anyhow::anyhow!("source receive failure").context("test item receive failure")
        ))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), epoch.changed())
        .await
        .expect("item error should invalidate epoch")
        .unwrap();
    assert_eq!(backend.subscribe_calls(), 1);

    sender
        .send(Ok(control_event(tenant.id, "after-item-error")))
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), local.recv())
        .await
        .expect("stream should continue after item error")
        .unwrap();
    assert_event_id(&received, "after-item-error");
    assert!(local.try_recv().is_err());
}

#[tokio::test]
async fn control_plane_eof_invalidates_epoch_then_resubscribes_after_one_second_once() {
    let (first_sender, first_stream) = control_stream();
    let (second_sender, second_stream) = control_stream();
    let backend = Arc::new(ScriptedControlPlaneBackend::new([
        SubscribeStep::Stream(first_stream),
        SubscribeStep::Stream(second_stream),
    ]));
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::for_tests(backend.clone()));
    let tenant = state
        .tenants()
        .create("eof-acme", "EOF Acme")
        .await
        .unwrap();
    let mut local = state.printer_events().subscribe(tenant.id).await;
    let mut epoch = state.printer_events().subscribe_epoch();
    let (_task, ready) = spawn_control_plane_ready(state);
    ready.await.unwrap().unwrap();

    let eof_at = Instant::now();
    drop(first_sender);
    tokio::time::timeout(Duration::from_secs(1), epoch.changed())
        .await
        .expect("EOF should invalidate epoch")
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(backend.subscribe_calls(), 1);
    wait_for_subscriptions(&backend, 2).await;
    assert!(eof_at.elapsed() >= Duration::from_millis(950));

    second_sender
        .send(Ok(control_event(tenant.id, "after-eof")))
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), local.recv())
        .await
        .expect("resubscribed stream should deliver")
        .unwrap();
    assert_event_id(&received, "after-eof");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        local.try_recv().is_err(),
        "resubscribe must not duplicate events"
    );
}

#[tokio::test]
async fn control_plane_subscribe_failure_invalidates_and_retries_after_one_second() {
    let (sender, stream) = control_stream();
    let backend = Arc::new(ScriptedControlPlaneBackend::new([
        SubscribeStep::Failure(
            anyhow::anyhow!("source subscribe failure").context("scripted subscribe failure"),
        ),
        SubscribeStep::Stream(stream),
    ]));
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::for_tests(backend.clone()));
    let tenant = state
        .tenants()
        .create("subscribe-retry-acme", "Subscribe Retry Acme")
        .await
        .unwrap();
    let mut local = state.printer_events().subscribe(tenant.id).await;
    let mut epoch = state.printer_events().subscribe_epoch();
    let failed_at = Instant::now();
    let (_task, mut ready) = spawn_control_plane_ready(state);
    tokio::time::timeout(Duration::from_secs(1), epoch.changed())
        .await
        .expect("subscribe failure should invalidate epoch")
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(500), &mut ready)
            .await
            .is_err(),
        "readiness must remain pending until subscription succeeds"
    );
    assert_eq!(backend.subscribe_calls(), 1);
    wait_for_subscriptions(&backend, 2).await;
    assert!(failed_at.elapsed() >= Duration::from_millis(950));
    ready.await.unwrap().unwrap();

    sender
        .send(Ok(control_event(tenant.id, "after-subscribe-failure")))
        .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), local.recv())
        .await
        .expect("retried subscription should deliver")
        .unwrap();
    assert_event_id(&received, "after-subscribe-failure");
    assert!(local.try_recv().is_err());
}

#[tokio::test]
async fn mixed_replica_snapshots_deliver_once_each_without_a_new_event_variant() {
    let (sender, stream) = control_stream();
    let backend = Arc::new(ScriptedControlPlaneBackend::new([SubscribeStep::Stream(
        stream,
    )]));
    let state = AppState::sqlite_for_tests()
        .await
        .unwrap()
        .with_control_plane_for_tests(ControlPlane::for_tests(backend));
    let tenant = state
        .tenants()
        .create("mixed-replica-acme", "Mixed Replica Acme")
        .await
        .unwrap();
    let mut local = state.printer_events().subscribe(tenant.id).await;
    let (_task, ready) = spawn_control_plane_ready(state);
    ready.await.unwrap().unwrap();

    sender
        .send(Ok(snapshot_control_event(tenant.id, false)))
        .unwrap();
    sender
        .send(Ok(snapshot_control_event(tenant.id, true)))
        .unwrap();

    let legacy = local.recv().await.unwrap();
    let enriched = local.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer: legacy } = legacy else {
        panic!("expected legacy printer snapshot")
    };
    let PrinterEvent::PrinterSnapshot { printer: enriched } = enriched else {
        panic!("expected enriched printer snapshot")
    };
    assert_eq!(legacy.state_revision, None);
    assert_eq!(legacy.print, None);
    assert_eq!(enriched.state_revision, Some(7));
    assert!(enriched.print.is_some());
    assert!(local.try_recv().is_err());
}

fn control_stream() -> (
    mpsc::UnboundedSender<anyhow::Result<HubControlMessage>>,
    ControlMessageStream,
) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (sender, Box::pin(UnboundedReceiverStream::new(receiver)))
}

async fn wait_for_subscriptions(backend: &ScriptedControlPlaneBackend, expected: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while backend.subscribe_calls() < expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("control plane should resubscribe after one second");
}

fn control_event(tenant_id: pandar_core::TenantId, id: &str) -> HubControlMessage {
    HubControlMessage::PrinterEvent {
        tenant_id: tenant_id.to_string(),
        event: test_event(id),
    }
}

fn snapshot_control_event(tenant_id: pandar_core::TenantId, enriched: bool) -> HubControlMessage {
    let mut fixture = serde_json::json!({
        "type": "printer_event",
        "tenant_id": tenant_id.to_string(),
        "event": {
            "type": "printer_snapshot",
            "printer": {
                "id": "printer-1",
                "tenant_id": tenant_id.to_string(),
                "agent_id": pandar_core::AgentId::new().to_string(),
                "serial_number": "SN-1",
                "name": "Printer",
                "model": null,
                "status": "RUNNING",
                "last_seen_at": "2026-07-10T00:00:00Z",
                "created_at": "2026-07-10T00:00:00Z",
                "nozzle_temperatures": [],
                "active_nozzle": null,
                "bed_temperature_celsius": null,
                "bed_target_temperature_celsius": null,
                "chamber_temperature_celsius": null,
                "chamber_light_on": null,
                "materials": null
            }
        }
    });
    if enriched {
        let printer = fixture["event"]["printer"].as_object_mut().unwrap();
        printer.insert("state_revision".to_owned(), serde_json::json!(7));
        printer.insert(
            "print".to_owned(),
            serde_json::json!({
                "task_generation": 1,
                "error_generation": 0,
                "job_state": null,
                "gcode_state": "RUNNING",
                "task_id": null,
                "subtask_id": null,
                "progress_percent": null,
                "remaining_time_minutes": null,
                "current_layer": null,
                "total_layers": null,
                "gcode_file": null,
                "subtask_name": null,
                "print_error": null,
                "printer_job_id": null,
                "hms": []
            }),
        );
    }
    serde_json::from_value(fixture).unwrap()
}

fn test_event(id: &str) -> PrinterEvent {
    PrinterEvent::CommandResult {
        command: Box::new(PrinterEventCommand {
            id: id.to_owned(),
            tenant_id: "tenant".to_owned(),
            agent_id: "agent".to_owned(),
            printer_id: None,
            kind: "test".to_owned(),
            status: "succeeded".to_owned(),
            payload_json: "{}".to_owned(),
            error: None,
            result_json: None,
            created_at: "2026-07-10T00:00:00Z".to_owned(),
            updated_at: "2026-07-10T00:00:00Z".to_owned(),
        }),
    }
}

fn assert_event_id(event: &PrinterEvent, expected: &str) {
    let PrinterEvent::CommandResult { command } = event else {
        panic!("expected command result")
    };
    assert_eq!(command.id, expected);
}
