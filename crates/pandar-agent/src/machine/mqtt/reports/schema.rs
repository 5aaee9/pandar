use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

#[derive(Debug, Default, Deserialize)]
pub(super) struct PrintReportEnvelope {
    #[serde(default)]
    pub(super) print: PrintReportSection,
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

#[derive(Debug, Default, Deserialize)]
pub(super) struct HmsEnvelope {
    #[serde(default)]
    pub(super) print: HmsPrint,
    #[serde(flatten)]
    pub(super) fields: BTreeMap<String, HmsValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct HmsPrint {
    #[serde(flatten)]
    pub(super) fields: BTreeMap<String, HmsValue>,
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

    pub(super) fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
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
