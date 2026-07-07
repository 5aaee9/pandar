use serde::Deserialize;
use serde_json::{Number, Value};

pub(super) struct OperationReport {
    raw: Value,
    envelope: OperationEnvelope,
}

#[derive(Debug, Deserialize)]
struct OperationEnvelope {
    print: Option<OperationSection>,
    info: Option<OperationSection>,
    pushing: Option<OperationSection>,
    system: Option<OperationSection>,
    camera: Option<OperationSection>,
}

#[derive(Debug, Deserialize)]
struct OperationSection {
    sequence_id: Option<SequenceId>,
    result: Option<String>,
    err_code: Option<i64>,
    errno: Option<i64>,
    reason: Option<String>,
    message: Option<String>,
    msg: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SequenceId {
    String(String),
    Number(Number),
}

impl OperationReport {
    pub(super) fn from_value(raw: Value) -> Option<Self> {
        let envelope = serde_json::from_value(raw.clone()).ok()?;
        Some(Self { raw, envelope })
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
        if section.result.as_deref() == Some("fail") {
            return Some(
                report_error_message(section)
                    .unwrap_or_else(|| "printer reported failure".to_owned()),
            );
        }
        for (key, code) in [("err_code", section.err_code), ("errno", section.errno)] {
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

    pub(super) fn into_raw(self) -> Value {
        self.raw
    }
}

fn report_error_message(section: &OperationSection) -> Option<String> {
    [
        section.reason.as_deref(),
        section.message.as_deref(),
        section.msg.as_deref(),
        section.error.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .map(ToOwned::to_owned)
}

impl OperationSection {
    fn sequence_id_string(&self) -> Option<String> {
        self.sequence_id.as_ref().map(SequenceId::as_string)
    }
}

impl SequenceId {
    fn as_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => value.to_string(),
        }
    }
}
