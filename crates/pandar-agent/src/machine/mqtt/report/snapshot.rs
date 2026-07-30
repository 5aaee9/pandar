use anyhow::{Context, anyhow};
use pandar_core::BambuDeviceFeatures;
use serde::Deserialize;
use serde_json::Number;

#[derive(Debug, Default)]
pub(in crate::machine::mqtt) enum FunField {
    #[default]
    Missing,
    String(String),
    Invalid,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PresentFun {
    String(String),
    Invalid(serde::de::IgnoredAny),
}

pub(in crate::machine::mqtt) fn deserialize_fun_field<'de, D>(
    deserializer: D,
) -> Result<FunField, D::Error>
where
    D: serde::Deserializer<'de>,
{
    PresentFun::deserialize(deserializer).map(|value| match value {
        PresentFun::String(value) => FunField::String(value),
        PresentFun::Invalid(_) => FunField::Invalid,
    })
}

pub(crate) fn device_feature_observation(
    serial: &str,
    report: &SnapshotReport,
) -> anyhow::Result<Option<BambuDeviceFeatures>> {
    match &report.print.as_ref().map(|print| &print.fun) {
        None | Some(FunField::Missing) => Ok(None),
        Some(FunField::String(value)) => BambuDeviceFeatures::from_hex(value)
            .with_context(|| format!("parse printer {serial} print.fun"))
            .map(Some),
        Some(FunField::Invalid) => Err(anyhow!(
            "printer {serial} print.fun expected a hexadecimal string"
        )),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SnapshotReport {
    #[serde(default)]
    pub(in crate::machine::mqtt) state: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) print: Option<SnapshotPrint>,
}

impl SnapshotReport {
    pub(crate) fn is_full_push_status(&self, expected_sequence_id: &str) -> bool {
        self.print.as_ref().is_some_and(|print| {
            print.command.as_deref() == Some("push_status")
                && print.msg == Some(0)
                && print.sequence_id.as_deref() == Some(expected_sequence_id)
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct SnapshotPrint {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    msg: Option<u8>,
    #[serde(default)]
    sequence_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_fun_field")]
    pub(in crate::machine::mqtt) fun: FunField,
    pub(in crate::machine::mqtt) gcode_state: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) state: Option<ScalarValue>,
    #[serde(default, alias = "bed_temp", alias = "bed_temperature")]
    pub(in crate::machine::mqtt) bed_temper: Option<TemperatureValue>,
    #[serde(default, alias = "target_bed_temper", alias = "bed_target_temperature")]
    pub(in crate::machine::mqtt) bed_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "chamber_temp", alias = "chamber_temperature")]
    pub(in crate::machine::mqtt) chamber_temper: Option<TemperatureValue>,
    #[serde(default, rename = "ctt")]
    pub(in crate::machine::mqtt) chamber_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "nozzle_temp", alias = "nozzle_temperature")]
    pub(in crate::machine::mqtt) nozzle_temper: Option<TemperatureValue>,
    #[serde(
        default,
        alias = "target_nozzle_temper",
        alias = "nozzle_target_temperature"
    )]
    pub(in crate::machine::mqtt) nozzle_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "right_nozzle_temper", alias = "nozzle_temp2")]
    pub(in crate::machine::mqtt) nozzle_temper2: Option<TemperatureValue>,
    #[serde(
        default,
        alias = "right_nozzle_target_temper",
        alias = "target_nozzle_temper2"
    )]
    pub(in crate::machine::mqtt) nozzle_target_temper2: Option<TemperatureValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) lights_report: Vec<LightReport>,
    #[serde(default)]
    pub(in crate::machine::mqtt) device: SnapshotDevice,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct SnapshotDevice {
    #[serde(default)]
    pub(in crate::machine::mqtt) bed_temp: Option<TemperatureValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) ctc: CtcDevice,
    #[serde(default)]
    pub(in crate::machine::mqtt) extruder: ExtruderDevice,
    #[serde(default)]
    pub(in crate::machine::mqtt) nozzle: NozzleDevice,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct CtcDevice {
    #[serde(default)]
    pub(in crate::machine::mqtt) info: CtcInfo,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct CtcInfo {
    #[serde(default)]
    pub(in crate::machine::mqtt) temp: Option<TemperatureValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct ExtruderDevice {
    #[serde(default)]
    pub(in crate::machine::mqtt) state: Option<u64>,
    #[serde(default)]
    pub(in crate::machine::mqtt) info: Vec<ExtruderInfo>,
}

#[derive(Debug, Deserialize)]
pub(in crate::machine::mqtt) struct ExtruderInfo {
    #[serde(default)]
    pub(in crate::machine::mqtt) id: Option<u64>,
    #[serde(default)]
    pub(in crate::machine::mqtt) temp: Option<TemperatureValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::machine::mqtt) struct NozzleDevice {
    #[serde(default)]
    pub(in crate::machine::mqtt) info: Vec<NozzleInfo>,
}

#[derive(Debug, Deserialize)]
pub(in crate::machine::mqtt) struct NozzleInfo {
    #[serde(default)]
    pub(in crate::machine::mqtt) id: Option<u64>,
    #[serde(default)]
    pub(in crate::machine::mqtt) diameter: Option<ScalarValue>,
    #[serde(default)]
    pub(in crate::machine::mqtt) nozzle_type: Option<String>,
    #[serde(default, rename = "type")]
    pub(in crate::machine::mqtt) kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::machine::mqtt) struct LightReport {
    #[serde(default)]
    pub(in crate::machine::mqtt) node: Option<String>,
    #[serde(default)]
    pub(in crate::machine::mqtt) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum TemperatureValue {
    Number(Number),
    String(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::machine::mqtt) enum ScalarValue {
    Number(Number),
    String(String),
}
