use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::super::MachineReportDiagnosticPayload;

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
    pub(super) print_error: Option<DiagnosticValue>,
    #[serde(flatten)]
    pub(super) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum NumericValue {
    Number(Number),
    String(String),
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

impl DiagnosticObject {
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
