use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use super::*;
use crate::{
    connection::{AuthDisposition, ConnectionSession, Reachability},
    studio_status::{PrinterObservation, project_stream_device},
};

fn observation(dev_id: &str, printer_id: &str, online: bool) -> PrinterObservation {
    let raw = format!(
        r#"{{"dev_id":"{dev_id}","dev_name":"P","name":"P","dev_model_name":"N1","model":"N1","dev_online":{online},"online":{online},"task_status":"idle","state":"idle","hms":[],"pandar_printer_id":"{printer_id}","nozzle_temperatures":[],"active_nozzle":null}}"#
    );
    project_stream_device(&raw).expect("device projects")
}

fn hermetic_session() -> (ConnectionSession, Arc<StdMutex<ConnectionState>>, Fence) {
    let session = ConnectionSession::new("http://127.0.0.1:1".to_owned(), "token".to_owned());
    {
        let mut state = session.state.lock().unwrap();
        state.tenant_id = "tenant-1".to_owned();
    }
    let state = Arc::clone(&session.state);
    (
        session,
        state,
        Fence {
            generation: 0,
            account_epoch: 0,
        },
    )
}

#[test]
fn backoff_sequence_is_bounded() {
    let delays = (0..8)
        .map(|attempt| backoff_delay(attempt).as_millis() as u64)
        .collect::<Vec<_>>();
    assert_eq!(
        delays,
        vec![1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]
    );
}

#[test]
fn printer_events_url_preserves_base_path_and_encodes_tenant() {
    assert_eq!(
        printer_events_url("http://hub.example.test", "tenant-1").unwrap(),
        "ws://hub.example.test/api/v1/tenants/tenant-1/printer-events?projection=studio&version=1"
    );
    assert_eq!(
        printer_events_url("https://hub.example.test/pandar", "tenant / one").unwrap(),
        "wss://hub.example.test/pandar/api/v1/tenants/tenant%20%2F%20one/printer-events?projection=studio&version=1"
    );
    assert!(printer_events_url("ftp://hub.example.test", "tenant").is_none());
}

#[test]
fn upsert_coalesces_selected_cloud_status_and_advances_cache_fence() {
    let (session, state, fence) = hermetic_session();
    session.studio_set_selected("serial-1".to_owned());
    session.studio_set_listener(crate::connection::studio::CLOUD_MESSAGE_LISTENER, true);
    {
        let mut shared = state.lock().unwrap();
        shared.reachability = Reachability::Connected;
        shared.auth = AuthDisposition::Accepted;
        shared.printers_fresh = true;
    }
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-1", "p1", true)
    ));
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-1", "p1", true)
    ));
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-2", "p2", true)
    ));

    let work = state.lock().unwrap().take_work();
    let status = work
        .iter()
        .filter(|item| item.kind == 1)
        .collect::<Vec<_>>();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].dev_id, "serial-1");
}

#[test]
fn newer_upsert_invalidates_an_issued_old_status_ticket() {
    let (session, state, fence) = hermetic_session();
    session.studio_set_selected("serial-1".to_owned());
    session.studio_set_listener(crate::connection::studio::CLOUD_MESSAGE_LISTENER, true);
    {
        let mut shared = state.lock().unwrap();
        shared.reachability = Reachability::Connected;
        shared.auth = AuthDisposition::Accepted;
        shared.printers_fresh = true;
        shared
            .printers
            .insert("serial-1".to_owned(), observation("serial-1", "p1", true));
    }
    let ticket = session
        .studio_prepare_message(0, "serial-1".to_owned(), 0, false, 0)
        .0
        .ticket;
    assert_ne!(ticket, 0);
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-1", "p1", true)
    ));
    assert!(!session.studio_claim_delivery(ticket));
}

#[test]
fn removal_requires_both_studio_and_pandar_identities() {
    let (_session, state, fence) = hermetic_session();
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-1", "p1", true)
    ));
    assert!(cache::apply_removal(&state, fence, "serial-1", "old"));
    assert!(state.lock().unwrap().printers.contains_key("serial-1"));
    assert!(cache::apply_removal(&state, fence, "serial-1", "p1"));
    assert!(!state.lock().unwrap().printers.contains_key("serial-1"));
}

#[test]
fn stale_projection_updates_the_cached_device_envelope() {
    let mut printer = observation("serial-1", "p1", true);
    printer.project_offline();
    let value = serde_json::from_str::<serde_json::Value>(&printer.raw_device).unwrap();
    assert_eq!(value["dev_online"], false);
    assert_eq!(value["online"], false);
    assert!(!printer.online);
}

#[test]
fn auth_rejection_clears_cache_and_pending_status() {
    let (session, state, fence) = hermetic_session();
    session.studio_set_selected("serial-1".to_owned());
    session.studio_set_listener(crate::connection::studio::CLOUD_MESSAGE_LISTENER, true);
    assert!(cache::apply_upsert(
        &state,
        fence,
        observation("serial-1", "p1", true)
    ));
    cache::apply_auth_rejected(&state, fence);
    let mut shared = state.lock().unwrap();
    assert!(shared.printers.is_empty());
    assert!(!shared.printers_fresh);
    assert!(shared.auth == AuthDisposition::Rejected);
    assert!(shared.take_work().is_empty());
}

#[test]
fn health_schedule_starts_at_grace_and_then_caps_at_thirty_seconds() {
    let started = Instant::now();
    let mut outage = Outage {
        next_health: started + OUTAGE_GRACE,
    };
    assert_eq!(outage.next_health.duration_since(started), OUTAGE_GRACE);
    outage.mark_health_done();
    let remaining = outage.next_health.saturating_duration_since(Instant::now());
    assert!(remaining <= HEALTH_INTERVAL);
    assert!(remaining > HEALTH_INTERVAL - Duration::from_millis(100));
}
