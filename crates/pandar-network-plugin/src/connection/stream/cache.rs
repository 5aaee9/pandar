use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use serde::Deserialize;

use super::{Fence, StreamSignals};
use crate::{
    connection::{
        AuthDisposition, ConnectionState, Reachability, RequestSnapshot, request::fetch_readiness,
    },
    studio_status::PrinterObservation,
};

pub(super) async fn observe_health(state: &Arc<Mutex<ConnectionState>>, snapshot: RequestSnapshot) {
    let fence = Fence {
        generation: snapshot.generation,
        account_epoch: snapshot.account_epoch,
    };
    let result = tokio::task::spawn_blocking(move || fetch_readiness(&snapshot))
        .await
        .context("join Hub health observation")
        .and_then(|observed| observed);
    let healthy = match result {
        Ok(response) if response.http_code == 200 => {
            match serde_json::from_str::<HealthResponse>(&response.body)
                .context("decode Hub health response")
            {
                Ok(body) => body.status == "ok",
                Err(error) => {
                    eprintln!("pandar Hub health observation failed: {error:#}");
                    false
                }
            }
        }
        Ok(response) => {
            eprintln!(
                "pandar Hub health observation failed: unexpected HTTP {}",
                response.http_code
            );
            false
        }
        Err(error) => {
            eprintln!("pandar Hub health observation failed: {error:#}");
            false
        }
    };

    let mut shared = state.lock().expect("connection state");
    if !fence.matches(&shared) {
        return;
    }
    if healthy {
        if shared.printers_fresh && shared.auth == AuthDisposition::Accepted {
            shared.set_reachability(Reachability::Connected);
        }
        if !shared.stream_degraded {
            shared.stream_degraded = true;
            shared.stream_error_pending = true;
            shared.printer_epoch = shared.printer_epoch.wrapping_add(1);
            shared.studio.invalidate_cache();
            let went_offline = shared
                .printers
                .values_mut()
                .filter(|printer| printer.online)
                .map(|printer| {
                    printer.project_offline();
                    printer.dev_id.clone()
                })
                .collect::<Vec<_>>();
            shared.queue_forced_offline(went_offline);
            shared.unconfirmed_online.clear();
        }
    } else {
        shared.set_reachability(Reachability::Disconnected);
    }
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
}

pub(super) fn apply_auth_rejected(state: &Arc<Mutex<ConnectionState>>, fence: Fence) {
    let mut shared = state.lock().expect("connection state");
    if !fence.matches(&shared) {
        return;
    }
    shared.printer_epoch = shared.printer_epoch.wrapping_add(1);
    shared.printers_fresh = false;
    shared.printers.clear();
    shared.stream_degraded = false;
    shared.stream_error_pending = false;
    shared.clear_deliveries();
    shared.studio.clear_stream_work();
    shared.reject_auth();
}

pub(super) fn apply_snapshot(
    state: &Arc<Mutex<ConnectionState>>,
    signals: &StreamSignals,
    fence: Fence,
    printers: Vec<PrinterObservation>,
) -> bool {
    let mut shared = state.lock().expect("connection state");
    if !fence.matches(&shared) {
        return false;
    }
    let next = printers
        .into_iter()
        .map(|printer| (printer.dev_id.clone(), printer))
        .collect::<BTreeMap<_, _>>();
    let confirmed_online = next
        .values()
        .filter(|printer| printer.online)
        .map(|printer| printer.dev_id.clone())
        .collect::<BTreeSet<_>>();
    let replaced = shared
        .printers
        .iter()
        .filter(|(dev_id, previous)| {
            next.get(*dev_id)
                .is_some_and(|current| current.pandar_printer_id != previous.pandar_printer_id)
        })
        .map(|(dev_id, _)| dev_id.clone())
        .collect::<BTreeSet<_>>();
    let mut offline = shared
        .printers
        .values()
        .filter(|previous| previous.online)
        .filter(|previous| {
            replaced.contains(&previous.dev_id) || !confirmed_online.contains(&previous.dev_id)
        })
        .map(|previous| previous.dev_id.clone())
        .collect::<BTreeSet<_>>();
    offline.extend(shared.unconfirmed_online.iter().cloned());
    for dev_id in &replaced {
        offline.remove(dev_id);
    }

    shared.printer_epoch = shared.printer_epoch.wrapping_add(1);
    shared.studio.invalidate_cache();
    shared.queue_offline(offline);
    shared.queue_forced_offline(replaced.iter().cloned());
    for dev_id in confirmed_online.difference(&replaced) {
        shared.recover_online(dev_id);
    }
    shared.unconfirmed_online.clear();
    shared.printers = next;
    shared.printers_fresh = true;
    shared.stream_degraded = false;
    shared.accept_auth();
    shared.set_reachability(Reachability::Connected);
    let statuses = shared
        .printers
        .values()
        .filter(|printer| printer.online)
        .map(|printer| (printer.dev_id.clone(), printer.status_report.clone()))
        .collect::<Vec<_>>();
    for (dev_id, body) in statuses {
        shared.studio.queue_status(dev_id, body);
    }
    drop(shared);
    signals.notify_snapshot();
    true
}

pub(super) fn apply_upsert(
    state: &Arc<Mutex<ConnectionState>>,
    fence: Fence,
    observation: PrinterObservation,
) -> bool {
    let mut shared = state.lock().expect("connection state");
    if !fence.matches(&shared) {
        return false;
    }
    let dev_id = observation.dev_id.clone();
    let replaced = shared
        .printers
        .get(&dev_id)
        .is_some_and(|previous| previous.pandar_printer_id != observation.pandar_printer_id);
    let went_offline = shared
        .printers
        .get(&dev_id)
        .is_some_and(|previous| previous.online && !observation.online);

    shared.printer_epoch = shared.printer_epoch.wrapping_add(1);
    shared.studio.invalidate_cache();
    if replaced {
        shared.queue_forced_offline([dev_id.clone()]);
    } else if went_offline {
        shared.queue_offline([dev_id.clone()]);
    } else if observation.online {
        shared.recover_online(&dev_id);
    }
    let status_report = observation.status_report.clone();
    let online = observation.online;
    shared.printers.insert(dev_id.clone(), observation);
    if online {
        shared.studio.queue_status(dev_id, status_report);
    }
    true
}

pub(super) fn apply_removal(
    state: &Arc<Mutex<ConnectionState>>,
    fence: Fence,
    dev_id: &str,
    pandar_printer_id: &str,
) -> bool {
    let dev_id = crate::connection::normalize_studio_dev_id(dev_id.to_owned());
    let mut shared = state.lock().expect("connection state");
    if !fence.matches(&shared) {
        return false;
    }
    let Some(current) = shared.printers.get(&dev_id) else {
        return true;
    };
    if current.pandar_printer_id != pandar_printer_id {
        return true;
    }
    shared.printer_epoch = shared.printer_epoch.wrapping_add(1);
    shared.studio.invalidate_cache();
    shared.queue_forced_offline([dev_id.clone()]);
    shared.printers.remove(&dev_id);
    true
}
