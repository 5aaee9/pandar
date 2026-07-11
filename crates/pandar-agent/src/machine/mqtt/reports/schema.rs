use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::super::{MachineHmsItem, MachineReportDiagnostic, MachineReportDiagnosticPayload};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PrintReportEnvelope {
    #[serde(default)]
    pub(super) print: PrintReportSection,
    #[serde(flatten)]
    pub(super) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct PrintReportSection {
    #[serde(default)]
    pub(super) task_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_printer_job_id")]
    pub(super) job_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_job_attr")]
    pub(super) job_attr: Option<NumericValue>,
    #[serde(default)]
    pub(super) subtask_id: Option<String>,
    #[serde(default)]
    pub(super) gcode_state: Option<String>,
    #[serde(default)]
    pub(super) mc_percent: Option<NumericValue>,
    #[serde(default)]
    pub(super) mc_remaining_time: Option<NumericValue>,
    #[serde(default)]
    pub(super) layer_num: Option<NumericValue>,
    #[serde(default)]
    pub(super) total_layer_num: Option<NumericValue>,
    #[serde(default)]
    pub(super) gcode_file: Option<String>,
    #[serde(default)]
    pub(super) subtask_name: Option<String>,
    #[serde(default)]
    pub(super) print_error: Option<PrintErrorValue>,
    #[serde(default)]
    pub(super) hms: Option<Vec<PrintHmsItem>>,
    #[serde(flatten)]
    pub(super) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum NumericValue {
    Number(Number),
    String(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum PrintErrorValue {
    Number(Number),
    Diagnostic(DiagnosticValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum DiagnosticValue {
    Object(DiagnosticObject),
    String(String),
    Other(ReportJson),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum HmsValue {
    Array(Vec<HmsValue>),
    Object(DiagnosticObject),
    Other(ReportJson),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum PrintHmsItem {
    Machine(MachineHmsItem),
    Legacy(LegacyHmsDiagnostic),
    Unknown(ReportJson),
}

#[derive(Debug, Deserialize)]
pub(super) struct LegacyHmsDiagnostic {
    code: String,
    message: String,
    #[serde(flatten)]
    extra: BTreeMap<String, ReportJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DiagnosticObject {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    hms_code: Option<String>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    info: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, ReportJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum ReportJson {
    Object(BTreeMap<String, ReportJson>),
    Array(Vec<ReportJson>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl DiagnosticValue {
    pub(super) fn message(&self) -> Option<String> {
        match self {
            Self::Object(object) => object.message(),
            Self::String(raw) => trimmed_string(Some(raw)),
            Self::Other(ReportJson::Null) => None,
            Self::Other(value) => json_text(value),
        }
    }

    pub(super) fn payload(&self) -> MachineReportDiagnosticPayload {
        match self {
            Self::Object(object) => object.payload(),
            Self::String(value) => MachineReportDiagnosticPayload::String(value.clone()),
            Self::Other(value) => MachineReportDiagnosticPayload::from(value.clone()),
        }
    }
}

impl PrintErrorValue {
    pub(super) fn state(&self) -> Option<u32> {
        match self {
            Self::Number(number) => studio_print_error(number),
            Self::Diagnostic(_) => None,
        }
    }

    pub(super) fn diagnostic(&self) -> Option<&DiagnosticValue> {
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

impl HmsValue {
    pub(super) fn collect_objects<'a>(&'a self, objects: &mut Vec<&'a DiagnosticObject>) {
        match self {
            Self::Array(values) => {
                for value in values {
                    value.collect_objects(objects);
                }
            }
            Self::Object(object) => objects.push(object),
            Self::Other(value) => {
                let _ = value;
            }
        }
    }
}

impl PrintHmsItem {
    pub(super) fn machine(&self) -> Option<MachineHmsItem> {
        match self {
            Self::Machine(item) => Some(*item),
            Self::Legacy(_) | Self::Unknown(_) => None,
        }
    }

    pub(super) fn diagnostic(&self) -> Option<MachineReportDiagnostic> {
        match self {
            Self::Machine(item) => {
                let payload = BTreeMap::from([
                    (
                        "attr".to_owned(),
                        MachineReportDiagnosticPayload::Number(Number::from(item.attr)),
                    ),
                    (
                        "code".to_owned(),
                        MachineReportDiagnosticPayload::Number(Number::from(item.code)),
                    ),
                ]);
                Some(MachineReportDiagnostic {
                    kind: "hms".to_owned(),
                    severity: "warning".to_owned(),
                    code: Some(format!("{:04X}", item.code)),
                    message: String::new(),
                    payload: MachineReportDiagnosticPayload::Object(payload),
                })
            }
            Self::Legacy(legacy) => legacy.diagnostic(),
            Self::Unknown(value) => {
                let _ = value;
                None
            }
        }
    }
}

impl LegacyHmsDiagnostic {
    fn diagnostic(&self) -> Option<MachineReportDiagnostic> {
        let code = trimmed_string(Some(&self.code))?;
        let message = trimmed_string(Some(&self.message))?;
        let mut payload = BTreeMap::from([
            (
                "code".to_owned(),
                MachineReportDiagnosticPayload::String(self.code.clone()),
            ),
            (
                "message".to_owned(),
                MachineReportDiagnosticPayload::String(self.message.clone()),
            ),
        ]);
        payload.extend(self.extra.iter().map(|(key, value)| {
            (
                key.clone(),
                MachineReportDiagnosticPayload::from(value.clone()),
            )
        }));
        Some(MachineReportDiagnostic {
            kind: "hms".to_owned(),
            severity: "warning".to_owned(),
            code: Some(code),
            message,
            payload: MachineReportDiagnosticPayload::Object(payload),
        })
    }
}

impl DiagnosticObject {
    pub(super) fn diagnostic(&self) -> Option<MachineReportDiagnostic> {
        Some(MachineReportDiagnostic {
            kind: "hms".to_owned(),
            severity: "warning".to_owned(),
            code: Some(self.code()?),
            message: self.message()?,
            payload: self.payload(),
        })
    }

    pub(super) fn payload(&self) -> MachineReportDiagnosticPayload {
        let mut fields = BTreeMap::new();
        insert_optional_string(&mut fields, "code", self.code.as_deref());
        insert_optional_string(&mut fields, "hms_code", self.hms_code.as_deref());
        insert_optional_string(&mut fields, "error_code", self.error_code.as_deref());
        insert_optional_string(&mut fields, "message", self.message.as_deref());
        insert_optional_string(&mut fields, "msg", self.msg.as_deref());
        insert_optional_string(&mut fields, "description", self.description.as_deref());
        insert_optional_string(&mut fields, "info", self.info.as_deref());
        fields.extend(self.extra.iter().map(|(key, value)| {
            (
                key.clone(),
                MachineReportDiagnosticPayload::from(value.clone()),
            )
        }));
        MachineReportDiagnosticPayload::Object(fields)
    }

    pub(super) fn code(&self) -> Option<String> {
        trimmed_string(
            self.code
                .as_deref()
                .or(self.hms_code.as_deref())
                .or(self.error_code.as_deref()),
        )
    }

    pub(super) fn message(&self) -> Option<String> {
        trimmed_string(
            self.message
                .as_deref()
                .or(self.msg.as_deref())
                .or(self.description.as_deref())
                .or(self.info.as_deref()),
        )
    }
}

impl From<ReportJson> for MachineReportDiagnosticPayload {
    fn from(value: ReportJson) -> Self {
        match value {
            ReportJson::Object(object) => Self::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            ReportJson::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            ReportJson::String(value) => Self::String(value),
            ReportJson::Number(value) => Self::Number(value),
            ReportJson::Bool(value) => Self::Bool(value),
            ReportJson::Null => Self::Null,
        }
    }
}

fn insert_optional_string(
    fields: &mut BTreeMap<String, MachineReportDiagnosticPayload>,
    key: &'static str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        fields.insert(
            key.to_owned(),
            MachineReportDiagnosticPayload::String(value.to_owned()),
        );
    }
}

fn trimmed_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_text(value: &impl Serialize) -> Option<String> {
    serde_json::to_string(value)
        .ok()
        .filter(|message| !message.is_empty())
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
