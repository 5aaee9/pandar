use std::time::{SystemTime, UNIX_EPOCH};

use super::super::*;
use crate::connection::ConnectionState;

const CLOUD_OFFLINE_WORK: i32 = 1;
const STUDIO_WORK_CLOUD_MESSAGE: i32 = 1;
const STUDIO_WORK_LOCAL_MESSAGE: i32 = 2;
const STUDIO_WORK_PRINTER_CONNECTED: i32 = 4;
const LOCAL_OFFLINE_WORK: i32 = 2;
const LOCAL_LOST_WORK: i32 = 3;

impl ConnectionState {
    pub(in crate::connection) fn take_work(&mut self) -> Vec<StudioWork> {
        let pending = std::mem::take(&mut self.studio.pending_offline);
        let body = disconnect_json();
        let mut work = Vec::new();
        for (dev_id, offline) in pending {
            if offline.cloud_allowed
                && self.studio.listeners.cloud_message
                && self.studio.cloud_target(&dev_id)
            {
                let delivery = self.studio.issue(
                    DeliveryKind::CloudOffline {
                        forced: offline.forced,
                    },
                    dev_id.clone(),
                    self.account_epoch,
                    self.printer_epoch,
                );
                work.push(StudioWork {
                    kind: CLOUD_OFFLINE_WORK,
                    state: 0,
                    ticket: delivery.ticket,
                    generation: 0,
                    dev_id: dev_id.clone(),
                    body: body.clone(),
                });
            }
            if let Some(generation) = offline.local_generation {
                if self.studio.listeners.local_message {
                    let delivery = self.studio.issue(
                        DeliveryKind::LocalOffline { generation },
                        dev_id.clone(),
                        self.account_epoch,
                        self.printer_epoch,
                    );
                    work.push(StudioWork {
                        kind: LOCAL_OFFLINE_WORK,
                        state: 0,
                        ticket: delivery.ticket,
                        generation,
                        dev_id: dev_id.clone(),
                        body: body.clone(),
                    });
                }
                if self.studio.listeners.local_connected {
                    let delivery = self.studio.issue(
                        DeliveryKind::LocalLost { generation },
                        dev_id.clone(),
                        self.account_epoch,
                        self.printer_epoch,
                    );
                    work.push(StudioWork {
                        kind: LOCAL_LOST_WORK,
                        state: 2,
                        ticket: delivery.ticket,
                        generation,
                        dev_id,
                        body: body.clone(),
                    });
                }
            }
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock predates Unix epoch")
            .as_millis() as u64;
        for dev_id in std::mem::take(&mut self.studio.pending_connected) {
            let (delivery, payload) = self.prepare_connected(dev_id.clone(), now_ms);
            if delivery.status != 0 || delivery.ticket == 0 {
                continue;
            }
            let Some(payload) = payload else {
                continue;
            };
            work.push(StudioWork {
                kind: STUDIO_WORK_PRINTER_CONNECTED,
                state: 0,
                ticket: delivery.ticket,
                generation: 0,
                dev_id,
                body: payload.body,
            });
        }
        for (dev_id, body) in std::mem::take(&mut self.studio.pending_cloud_status) {
            if !self.studio.listeners.cloud_message
                || !self.studio.cloud_target(&dev_id)
                || !self.studio_eligible(&dev_id)
            {
                continue;
            }
            let initialize_cloud = !self.studio.cloud_initialized.contains(&dev_id);
            let delivery = self.studio.issue(
                DeliveryKind::Message {
                    tunnel: CLOUD_TUNNEL,
                    local_generation: 0,
                    initialize_cloud,
                },
                dev_id.clone(),
                self.account_epoch,
                self.printer_epoch,
            );
            work.push(StudioWork {
                kind: STUDIO_WORK_CLOUD_MESSAGE,
                state: 0,
                ticket: delivery.ticket,
                generation: 0,
                dev_id,
                body,
            });
        }
        for (dev_id, body) in std::mem::take(&mut self.studio.pending_local_status) {
            let generation = self.studio.local.generation;
            if !self.studio.listeners.local_message
                || !self.studio_eligible(&dev_id)
                || !self.studio.local.connected
                || self.studio.local.target.as_deref() != Some(&dev_id)
            {
                continue;
            }
            let delivery = self.studio.issue(
                DeliveryKind::Message {
                    tunnel: LOCAL_TUNNEL,
                    local_generation: generation,
                    initialize_cloud: false,
                },
                dev_id.clone(),
                self.account_epoch,
                self.printer_epoch,
            );
            work.push(StudioWork {
                kind: STUDIO_WORK_LOCAL_MESSAGE,
                state: 0,
                ticket: delivery.ticket,
                generation,
                dev_id,
                body,
            });
        }
        work
    }
}
