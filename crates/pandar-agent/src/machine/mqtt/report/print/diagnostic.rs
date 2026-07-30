use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::super::super::{
    MachineHmsItem, MachineReportDiagnostic, MachineReportDiagnosticPayload,
};
use super::ReportJson;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum DiagnosticValue {
    Object(DiagnosticObject),
    String(String),
    Other(ReportJson),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum HmsValue {
    Array(Vec<HmsValue>),
    Object(DiagnosticObject),
    Other(ReportJson),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum PrintHmsItem {
    Machine(MachineHmsItem),
    Legacy(LegacyHmsDiagnostic),
    Unknown(ReportJson),
}

#[derive(Debug, Deserialize)]
pub(in crate::machine::mqtt) struct LegacyHmsDiagnostic {
    code: String,
    message: String,
    #[serde(flatten)]
    extra: BTreeMap<String, ReportJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::machine::mqtt) struct DiagnosticObject {
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

impl DiagnosticValue {
    pub(in crate::machine::mqtt) fn message(&self) -> Option<String> {
        match self {
            Self::Object(object) => object.message(),
            Self::String(raw) => trimmed_string(Some(raw)),
            Self::Other(ReportJson::Null) => None,
            Self::Other(value) => json_text(value),
        }
    }

    pub(in crate::machine::mqtt) fn payload(&self) -> MachineReportDiagnosticPayload {
        match self {
            Self::Object(object) => object.payload(),
            Self::String(value) => MachineReportDiagnosticPayload::String(value.clone()),
            Self::Other(value) => MachineReportDiagnosticPayload::from(value.clone()),
        }
    }
}

impl HmsValue {
    pub(in crate::machine::mqtt) fn collect_objects<'a>(
        &'a self,
        objects: &mut Vec<&'a DiagnosticObject>,
    ) {
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
    pub(in crate::machine::mqtt) fn machine(&self) -> Option<MachineHmsItem> {
        match self {
            Self::Machine(item) => Some(*item),
            Self::Legacy(_) | Self::Unknown(_) => None,
        }
    }

    pub(in crate::machine::mqtt) fn diagnostic(&self) -> Option<MachineReportDiagnostic> {
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
    pub(in crate::machine::mqtt) fn diagnostic(&self) -> Option<MachineReportDiagnostic> {
        Some(MachineReportDiagnostic {
            kind: "hms".to_owned(),
            severity: "warning".to_owned(),
            code: Some(self.code()?),
            message: self.message()?,
            payload: self.payload(),
        })
    }

    pub(in crate::machine::mqtt) fn payload(&self) -> MachineReportDiagnosticPayload {
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

    pub(in crate::machine::mqtt) fn code(&self) -> Option<String> {
        trimmed_string(
            self.code
                .as_deref()
                .or(self.hms_code.as_deref())
                .or(self.error_code.as_deref()),
        )
    }

    pub(in crate::machine::mqtt) fn message(&self) -> Option<String> {
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
