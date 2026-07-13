use std::collections::BTreeMap;

use serde_json::Value;

use crate::machine::mqtt::{MachineReportDiagnostic, MachineReportDiagnosticPayload};

use super::schema::{HmsValue, NumericValue, PrintReportEnvelope};

pub(super) fn raw_print_payload(report: &Value) -> Option<MachineReportDiagnosticPayload> {
    report.get("print").map(value_payload)
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

pub(super) fn print_error_payload(
    print_error: MachineReportDiagnosticPayload,
    raw_print: Option<MachineReportDiagnosticPayload>,
) -> MachineReportDiagnosticPayload {
    let Some(raw_print) = raw_print else {
        return print_error;
    };

    let mut fields = BTreeMap::new();
    fields.insert("print_error".to_owned(), print_error);
    fields.insert("raw_print".to_owned(), raw_print);
    MachineReportDiagnosticPayload::Object(fields)
}

pub(super) fn trimmed_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn bounded_u32(value: Option<&NumericValue>, min: u32, max: u32) -> Option<u32> {
    let value = match value? {
        NumericValue::Number(number) => {
            if let Some(value) = number.as_u64() {
                u32::try_from(value).ok()?
            } else if let Some(value) = number.as_i64() {
                u32::try_from(value).ok()?
            } else {
                let value = number.as_f64()?;
                if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                    return None;
                }
                u32::try_from(value as u64).ok()?
            }
        }
        NumericValue::String(raw) => raw.trim().parse().ok()?,
    };

    (min..=max).contains(&value).then_some(value)
}

pub(super) fn collect_hms_diagnostics(
    envelope: &PrintReportEnvelope,
    diagnostics: &mut Vec<MachineReportDiagnostic>,
) {
    if let Some(hms) = &envelope.print.hms {
        diagnostics.extend(hms.iter().filter_map(|item| item.diagnostic()));
    }

    for fields in [&envelope.fields, &envelope.print.fields] {
        for value in hms_values(fields) {
            let mut objects = Vec::new();
            value.collect_objects(&mut objects);
            diagnostics.extend(objects.into_iter().filter_map(|object| object.diagnostic()));
        }
    }
}

fn hms_values(fields: &BTreeMap<String, HmsValue>) -> impl Iterator<Item = &HmsValue> {
    fields
        .iter()
        .filter(|(key, _)| key.to_ascii_lowercase().contains("hms"))
        .map(|(_, value)| value)
}
