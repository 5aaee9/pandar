use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

pub(in crate::machine::mqtt) mod diagnostic;

pub(in crate::machine::mqtt) use diagnostic::{DiagnosticValue, HmsValue, PrintHmsItem};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PrintReportEnvelope {
    #[serde(default)]
    pub(in crate::machine::mqtt) print: PrintReportSection,
    #[serde(flatten)]
    pub(in crate::machine::mqtt) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct PrintReportSection {
    #[serde(default)]
    pub(in crate::machine::mqtt) task_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_printer_job_id")]
    pub(in crate::machine::mqtt) job_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_job_attr")]
    pub(in crate::machine::mqtt) job_attr: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) subtask_id: Option<String>,
    #[serde(default)]
    pub(in crate::machine::mqtt) gcode_state: Option<String>,
    #[serde(default)]
    pub(in crate::machine::mqtt) state: Option<ReportJson>,
    #[serde(default)]
    pub(in crate::machine::mqtt) mc_percent: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) mc_remaining_time: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) spd_lvl: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) layer_num: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) total_layer_num: Option<NumericValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) gcode_file: Option<String>,
    #[serde(default)]
    pub(in crate::machine::mqtt) subtask_name: Option<String>,
    #[serde(default)]
    pub(in crate::machine::mqtt) print_error: Option<PrintErrorValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) hms: Option<Vec<PrintHmsItem>>,
    #[serde(flatten)]
    pub(in crate::machine::mqtt) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum NumericValue {
    Number(Number),
    String(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum PrintErrorValue {
    Number(Number),
    Diagnostic(DiagnosticValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum ReportJson {
    Object(BTreeMap<String, ReportJson>),
    Array(Vec<ReportJson>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl PrintErrorValue {
    pub(in crate::machine::mqtt) fn state(&self) -> Option<u32> {
        match self {
            Self::Number(number) => studio_print_error(number),
            Self::Diagnostic(_) => None,
        }
    }

    pub(in crate::machine::mqtt) fn diagnostic(&self) -> Option<&DiagnosticValue> {
        match self {
            Self::Diagnostic(value)
                if matches!(
                    value,
                    DiagnosticValue::Object(_) | DiagnosticValue::String(_)
                ) =>
            {
                Some(value)
            }
            Self::Number(_) | Self::Diagnostic(_) => None,
        }
    }
}

fn studio_print_error(number: &serde_json::Number) -> Option<u32> {
    if let Some(value) = number.as_i64() {
        return i32::try_from(value).ok().map(|value| value.max(0) as u32);
    }
    if let Some(value) = number.as_u64() {
        return i32::try_from(value).ok().map(|value| value as u32);
    }
    let value = number.as_f64()?;
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then(|| value.trunc().max(0.0) as u32)
}

fn studio_printer_job_id(value: &ReportJson) -> String {
    match value {
        ReportJson::String(value) => value.clone(),
        ReportJson::Number(number) if number.as_i64().is_some() => {
            number.as_i64().expect("checked above").to_string()
        }
        ReportJson::Number(number) if number.as_u64().is_some() => {
            i64::try_from(number.as_u64().expect("checked above"))
                .map(|value| value.to_string())
                .unwrap_or_default()
        }
        ReportJson::Number(number) => number
            .as_f64()
            .filter(|value| value.is_finite())
            .filter(|value| *value >= -9_223_372_036_854_775_808.0)
            .filter(|value| *value < 9_223_372_036_854_775_808.0)
            .map(|value| (value.trunc() as i64).to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn deserialize_printer_job_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = ReportJson::deserialize(deserializer)?;
    Ok(Some(studio_printer_job_id(&value)))
}

fn deserialize_job_attr<'de, D>(deserializer: D) -> Result<Option<NumericValue>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = ReportJson::deserialize(deserializer)?;
    Ok(match value {
        ReportJson::Number(value) => Some(NumericValue::Number(value)),
        ReportJson::String(value) => Some(NumericValue::String(value)),
        ReportJson::Object(_) | ReportJson::Array(_) | ReportJson::Bool(_) | ReportJson::Null => {
            None
        }
    })
}
