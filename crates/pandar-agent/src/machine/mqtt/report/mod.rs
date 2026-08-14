use std::time::Duration;

use serde_json::Value;

use super::{BambuMqttTransport, MachineReportDiagnosticPayload, PublishedMqttCommand};
use crate::machine::types::decode_json_payload_result;

mod firmware;
mod interpretation;
pub(in crate::machine) mod materials;
pub(in crate::machine::mqtt) mod print;
pub(in crate::machine::mqtt) mod snapshot;

pub(crate) use firmware::FirmwareReportReducer;
pub(crate) use interpretation::{
    DeviceFeatureObservations, MachineReportSectionDiagnostic, PrintTelemetryClass,
    SnapshotAuthority, SnapshotContent,
};
#[cfg(test)]
pub(crate) use interpretation::{MachineReportInterpretation, MachineReportSection};
pub(crate) use materials::MaterialsReport;
pub(crate) use print::PrintReportEnvelope;
pub(crate) use snapshot::SnapshotReport;

/// One Bambu MQTT report decoded once into typed sections. The raw payload is
/// retained privately for open-ended diagnostics pass-through only.
#[derive(Debug)]
pub(crate) struct MachineReport {
    raw: Value,
    print: DecodedSection<PrintReportEnvelope>,
    snapshot: DecodedSection<SnapshotReport>,
    materials: DecodedSection<MaterialsReport>,
}

#[derive(Debug)]
pub(super) enum DecodedSection<T> {
    Decoded(T),
    Invalid(serde_json::Error),
}

impl<T> DecodedSection<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    fn decode(value: &Value) -> Self {
        match decode_json_payload_result(value) {
            Ok(value) => Self::Decoded(value),
            Err(error) => Self::Invalid(error),
        }
    }
}

impl MachineReport {
    pub(crate) fn decode(value: Value) -> Self {
        Self {
            print: DecodedSection::decode(&value),
            snapshot: DecodedSection::decode(&value),
            materials: DecodedSection::decode(&value),
            raw: value,
        }
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
