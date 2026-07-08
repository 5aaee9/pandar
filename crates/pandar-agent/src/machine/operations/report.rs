use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Number, Value};

use crate::machine::{MachineJsonPayload, PrinterOperationMqttSummary, types::decode_json_payload};

pub(super) struct OperationReport {
    envelope: OperationEnvelope,
}

#[derive(Debug, Deserialize)]
struct OperationEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print: Option<OperationSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    info: Option<OperationSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pushing: Option<OperationSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system: Option<OperationSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    camera: Option<OperationSection>,
    #[serde(flatten)]
    extra: BTreeMap<String, MachineJsonPayload>,
}

#[derive(Debug, Deserialize)]
struct OperationSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sequence_id: Option<SequenceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    err_code: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    errno: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    msg: Option<MachineJsonPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<MachineJsonPayload>,
    #[serde(flatten)]
    extra: BTreeMap<String, MachineJsonPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SequenceId {
    String(String),
    Number(Number),
}

impl OperationReport {
    pub(super) fn from_payload(raw: &Value) -> Option<Self> {
        let envelope = decode_json_payload(raw)?;
        Some(Self { envelope })
    }

    pub(super) fn sequence_id(&self) -> Option<String> {
        [
            self.envelope.print.as_ref(),
            self.envelope.info.as_ref(),
            self.envelope.pushing.as_ref(),
            self.envelope.system.as_ref(),
            self.envelope.camera.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find_map(OperationSection::sequence_id_string)
    }

    pub(super) fn error(&self) -> Option<String> {
        let section = self
            .envelope
            .print
            .as_ref()
            .or(self.envelope.system.as_ref())?;
        if section.result.as_ref().and_then(payload_string).as_deref() == Some("fail") {
            return Some(
                report_error_message(section)
                    .unwrap_or_else(|| "printer reported failure".to_owned()),
            );
        }
        for (key, code) in [
            ("err_code", section.err_code.as_ref().and_then(payload_i64)),
            ("errno", section.errno.as_ref().and_then(payload_i64)),
        ] {
            if let Some(code) = code
                && code != 0
            {
                return Some(
                    report_error_message(section)
                        .unwrap_or_else(|| format!("printer reported {key} {code}")),
                );
            }
        }
        None
    }

    pub(super) fn summary(&self) -> Option<PrinterOperationMqttSummary> {
        let section = self
            .envelope
            .print
            .as_ref()
            .or(self.envelope.system.as_ref())?;
        Some(PrinterOperationMqttSummary {
            result: section.result.clone(),
            reason: section.reason.clone(),
            err_code: section.err_code.clone(),
            errno: section.errno.clone(),
        })
    }

    pub(super) fn into_payload(self) -> MachineJsonPayload {
        self.envelope.into_payload()
    }
}

fn report_error_message(section: &OperationSection) -> Option<String> {
    [
        section.reason.as_ref(),
        section.message.as_ref(),
        section.msg.as_ref(),
        section.error.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(payload_string)
    .map(|value| value.trim().to_owned())
    .find(|value| !value.is_empty())
}

impl OperationSection {
    fn sequence_id_string(&self) -> Option<String> {
        self.sequence_id.as_ref().map(SequenceId::as_string)
    }

    fn into_payload(self) -> MachineJsonPayload {
        let mut object = self.extra;
        insert_payload(
            &mut object,
            "sequence_id",
            self.sequence_id.map(SequenceId::into_payload),
        );
        insert_payload(&mut object, "result", self.result);
        insert_payload(&mut object, "err_code", self.err_code);
        insert_payload(&mut object, "errno", self.errno);
        insert_payload(&mut object, "reason", self.reason);
        insert_payload(&mut object, "message", self.message);
        insert_payload(&mut object, "msg", self.msg);
        insert_payload(&mut object, "error", self.error);
        MachineJsonPayload::Object(object)
    }
}

impl SequenceId {
    fn as_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => value.to_string(),
        }
    }

    fn into_payload(self) -> MachineJsonPayload {
        match self {
            Self::String(value) => MachineJsonPayload::String(value),
            Self::Number(value) => MachineJsonPayload::Number(value),
        }
    }
}

impl OperationEnvelope {
    fn into_payload(self) -> MachineJsonPayload {
        let mut object = self.extra;
        insert_section(&mut object, "print", self.print);
        insert_section(&mut object, "info", self.info);
        insert_section(&mut object, "pushing", self.pushing);
        insert_section(&mut object, "system", self.system);
        insert_section(&mut object, "camera", self.camera);
        MachineJsonPayload::Object(object)
    }
}

fn insert_section(
    object: &mut BTreeMap<String, MachineJsonPayload>,
    key: &str,
    section: Option<OperationSection>,
) {
    insert_payload(object, key, section.map(OperationSection::into_payload));
}

fn insert_payload(
    object: &mut BTreeMap<String, MachineJsonPayload>,
    key: &str,
    payload: Option<MachineJsonPayload>,
) {
    if let Some(payload) = payload {
        object.insert(key.to_owned(), payload);
    }
}

fn payload_string(payload: &MachineJsonPayload) -> Option<String> {
    match payload {
        MachineJsonPayload::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn payload_i64(payload: &MachineJsonPayload) -> Option<i64> {
    match payload {
        MachineJsonPayload::Number(value) => value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| value.try_into().ok())),
        _ => None,
    }
}
