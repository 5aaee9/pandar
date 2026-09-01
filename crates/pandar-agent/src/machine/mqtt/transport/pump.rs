use std::{collections::VecDeque, sync::Arc};

use rumqttc::{Event, EventLoop, Packet};
use serde_json::Value;
use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
};

use crate::machine::mqtt::decode_mqtt_report_payload;

const MQTT_REPORT_QUEUE_CAPACITY: usize = 32;

pub(super) struct MqttEventLoopPump {
    task: JoinHandle<()>,
    reports: Arc<MqttReportQueue>,
}

struct MqttReportQueue {
    serial: String,
    reports: Mutex<VecDeque<anyhow::Result<Value>>>,
    ready: Notify,
}

impl MqttReportQueue {
    fn new(serial: String) -> Self {
        Self {
            serial,
            reports: Mutex::new(VecDeque::with_capacity(MQTT_REPORT_QUEUE_CAPACITY)),
            ready: Notify::new(),
        }
    }

    async fn push(&self, report: anyhow::Result<Value>) {
        let mut reports = self.reports.lock().await;
        if reports.len() == MQTT_REPORT_QUEUE_CAPACITY {
            // Buffered reports can include incremental material patches whose
            // loss later frames cannot repair; surface the loss instead of
            // silently dropping entries so the consumer fails and requests a
            // fresh pushall resync.
            let dropped = reports.len();
            tracing::warn!(
                serial = %self.serial,
                dropped,
                capacity = MQTT_REPORT_QUEUE_CAPACITY,
                "MQTT report queue overflow; failing the report consumer for resync"
            );
            reports.clear();
            reports.push_back(Err(anyhow::anyhow!(
                "MQTT report queue for {} overflowed its capacity of {MQTT_REPORT_QUEUE_CAPACITY}; \
                 dropped {dropped} buffered reports",
                self.serial
            )));
        }
        reports.push_back(report);
        drop(reports);
        self.ready.notify_one();
    }

    async fn next(&self) -> anyhow::Result<Value> {
        loop {
            let ready = self.ready.notified();
            if let Some(report) = self.reports.lock().await.pop_front() {
                return report;
            }
            ready.await;
        }
    }

    #[cfg(test)]
    async fn wait_until_full(&self) {
        loop {
            let ready = self.ready.notified();
            if self.reports.lock().await.len() == MQTT_REPORT_QUEUE_CAPACITY {
                return;
            }
            ready.await;
        }
    }
}

impl MqttEventLoopPump {
    pub(super) fn spawn(event_loop: EventLoop, serial: String) -> Self {
        let reports = Arc::new(MqttReportQueue::new(serial));
        let task = tokio::spawn(run_event_loop(event_loop, Arc::clone(&reports)));
        Self { task, reports }
    }

    pub(super) async fn next_report(&self) -> anyhow::Result<Value> {
        self.reports.next().await
    }

    #[cfg(test)]
    pub(super) async fn wait_until_report_queue_full(&self) {
        self.reports.wait_until_full().await;
    }
}

impl Drop for MqttEventLoopPump {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run_event_loop(mut event_loop: EventLoop, reports: Arc<MqttReportQueue>) {
    loop {
        match event_loop.poll().await {
            Ok(event) => {
                if let Event::Incoming(Packet::Publish(publish)) = event {
                    reports
                        .push(decode_mqtt_report_payload(publish.payload.as_ref()))
                        .await;
                }
            }
            Err(error) => {
                reports
                    .push(Err(
                        anyhow::Error::new(error).context("poll rumqttc event loop")
                    ))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn report_queue_overflow_surfaces_an_error_instead_of_dropping_entries() {
        let reports = MqttReportQueue::new("SERIAL-OVERFLOW".to_owned());
        for sequence in 0..MQTT_REPORT_QUEUE_CAPACITY {
            reports
                .push(Ok(serde_json::json!({"sequence": sequence})))
                .await;
        }

        reports
            .push(Ok(serde_json::json!({
                "sequence": MQTT_REPORT_QUEUE_CAPACITY
            })))
            .await;

        let error = reports
            .next()
            .await
            .expect_err("overflow must fail the consumer for resync");
        assert!(error.to_string().contains("overflow"), "{error:#}");
        assert!(error.to_string().contains("SERIAL-OVERFLOW"), "{error:#}");

        assert_eq!(
            reports.next().await.unwrap()["sequence"],
            MQTT_REPORT_QUEUE_CAPACITY
        );
    }

    #[test]
    fn report_queue_overflow_warning_names_the_printer() {
        let (logs, ()) = crate::test_tracing::capture_logs(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let reports = MqttReportQueue::new("SERIAL-NAMED".to_owned());
                    for sequence in 0..MQTT_REPORT_QUEUE_CAPACITY {
                        reports
                            .push(Ok(serde_json::json!({"sequence": sequence})))
                            .await;
                    }

                    reports
                        .push(Ok(serde_json::json!({
                            "sequence": MQTT_REPORT_QUEUE_CAPACITY
                        })))
                        .await;
                });
        });

        let contents = logs.contents();
        assert!(
            contents.contains("MQTT report queue overflow"),
            "{contents}"
        );
        assert!(contents.contains("SERIAL-NAMED"), "{contents}");
        assert!(
            contents.contains(&format!("dropped={MQTT_REPORT_QUEUE_CAPACITY}")),
            "{contents}"
        );
    }
}
