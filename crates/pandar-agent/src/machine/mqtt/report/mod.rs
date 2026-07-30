use std::time::Duration;

use serde_json::Value;

use super::{BambuMqttTransport, MachineReportDiagnosticPayload, PublishedMqttCommand};
use crate::machine::types::decode_json_payload;

mod firmware;
pub(in crate::machine) mod materials;
pub(in crate::machine::mqtt) mod print;
pub(in crate::machine::mqtt) mod snapshot;

pub(crate) use firmware::FirmwareReportReducer;
pub(crate) use materials::MaterialsReport;
pub(crate) use print::PrintReportEnvelope;
pub(crate) use snapshot::{SnapshotReport, device_feature_observation};

/// One Bambu MQTT report decoded once into typed sections. The raw payload is
/// retained privately for open-ended diagnostics pass-through only.
#[derive(Debug)]
pub(crate) struct MachineReport {
    raw: Value,
    print: Option<PrintReportEnvelope>,
    snapshot: Option<SnapshotReport>,
    materials: Option<MaterialsReport>,
}

impl MachineReport {
    pub(crate) fn decode(value: Value) -> Self {
        Self {
            print: decode_json_payload(&value),
            snapshot: decode_json_payload(&value),
            materials: decode_json_payload(&value),
            raw: value,
        }
    }

    pub(crate) fn print(&self) -> Option<&PrintReportEnvelope> {
        self.print.as_ref()
    }

    pub(crate) fn snapshot(&self) -> Option<&SnapshotReport> {
        self.snapshot.as_ref()
    }

    pub(crate) fn materials(&self) -> Option<&MaterialsReport> {
        self.materials.as_ref()
    }

    pub(crate) fn device_feature_observation(
        &self,
        serial: &str,
    ) -> anyhow::Result<Option<pandar_core::BambuDeviceFeatures>> {
        match self.snapshot() {
            Some(report) => snapshot::device_feature_observation(serial, report),
            None => Ok(None),
        }
    }

    pub(crate) fn is_feature_only_report(&self) -> bool {
        self.raw.as_object().is_some_and(|fields| fields.len() == 1)
            && self
                .raw
                .get("print")
                .and_then(Value::as_object)
                .is_some_and(|fields| fields.len() == 1 && fields.contains_key("fun"))
    }

    pub(crate) fn raw_print_payload(&self) -> Option<MachineReportDiagnosticPayload> {
        self.raw.get("print").map(value_payload)
    }
}

fn value_payload(value: &Value) -> MachineReportDiagnosticPayload {
    match value {
        Value::Object(object) => MachineReportDiagnosticPayload::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), value_payload(value)))
                .collect(),
        ),
        Value::Array(values) => {
            MachineReportDiagnosticPayload::Array(values.iter().map(value_payload).collect())
        }
        Value::String(value) => MachineReportDiagnosticPayload::String(value.clone()),
        Value::Number(value) => MachineReportDiagnosticPayload::Number(value.clone()),
        Value::Bool(value) => MachineReportDiagnosticPayload::Bool(*value),
        Value::Null => MachineReportDiagnosticPayload::Null,
    }
}

/// Typed view over a [`BambuMqttTransport`]: reports cross the seam decoded,
/// commands pass through unchanged.
pub(crate) struct MachineReports<'a, T: BambuMqttTransport + ?Sized> {
    transport: &'a T,
}

impl<'a, T: BambuMqttTransport + ?Sized> MachineReports<'a, T> {
    pub(crate) fn new(transport: &'a T) -> Self {
        Self { transport }
    }

    pub(crate) async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        self.transport.subscribe(topic).await
    }

    pub(crate) async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()> {
        self.transport.publish(command).await
    }

    pub(crate) async fn next_report(&self, timeout: Duration) -> anyhow::Result<MachineReport> {
        let report = self.transport.next_report(timeout).await?;
        Ok(MachineReport::decode(report))
    }
}
