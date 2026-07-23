use serde::Deserialize;
use serde_json::Number;

use super::super::device_features::{FunField, deserialize_fun_field};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SnapshotReport {
    #[serde(default)]
    pub(super) state: Option<ScalarValue>,
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
    pub(super) gcode_state: Option<ScalarValue>,
    #[serde(default)]
    pub(super) state: Option<ScalarValue>,
    #[serde(default, alias = "bed_temp", alias = "bed_temperature")]
    pub(super) bed_temper: Option<TemperatureValue>,
    #[serde(default, alias = "target_bed_temper", alias = "bed_target_temperature")]
    pub(super) bed_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "chamber_temp", alias = "chamber_temperature")]
    pub(super) chamber_temper: Option<TemperatureValue>,
    #[serde(default, rename = "ctt")]
    pub(super) chamber_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "nozzle_temp", alias = "nozzle_temperature")]
    pub(super) nozzle_temper: Option<TemperatureValue>,
    #[serde(
        default,
        alias = "target_nozzle_temper",
        alias = "nozzle_target_temperature"
    )]
    pub(super) nozzle_target_temper: Option<TemperatureValue>,
    #[serde(default, alias = "right_nozzle_temper", alias = "nozzle_temp2")]
    pub(super) nozzle_temper2: Option<TemperatureValue>,
    #[serde(
        default,
        alias = "right_nozzle_target_temper",
        alias = "target_nozzle_temper2"
    )]
    pub(super) nozzle_target_temper2: Option<TemperatureValue>,
    #[serde(default)]
    pub(super) lights_report: Vec<LightReport>,
    #[serde(default)]
    pub(super) device: SnapshotDevice,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapshotDevice {
    #[serde(default)]
    pub(super) bed_temp: Option<TemperatureValue>,
    #[serde(default)]
    pub(super) ctc: CtcDevice,
    #[serde(default)]
    pub(super) extruder: ExtruderDevice,
    #[serde(default)]
    pub(super) nozzle: NozzleDevice,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct CtcDevice {
    #[serde(default)]
    pub(super) info: CtcInfo,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct CtcInfo {
    #[serde(default)]
    pub(super) temp: Option<TemperatureValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExtruderDevice {
    #[serde(default)]
    pub(super) state: Option<u64>,
    #[serde(default)]
    pub(super) info: Vec<ExtruderInfo>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ExtruderInfo {
    #[serde(default)]
    pub(super) id: Option<u64>,
    #[serde(default)]
    pub(super) temp: Option<TemperatureValue>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct NozzleDevice {
    #[serde(default)]
    pub(super) info: Vec<NozzleInfo>,
}

#[derive(Debug, Deserialize)]
pub(super) struct NozzleInfo {
    #[serde(default)]
    pub(super) id: Option<u64>,
    #[serde(default)]
    pub(super) diameter: Option<ScalarValue>,
    #[serde(default)]
    pub(super) nozzle_type: Option<String>,
    #[serde(default, rename = "type")]
    pub(super) kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LightReport {
    #[serde(default)]
    pub(super) node: Option<String>,
    #[serde(default)]
    pub(super) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum TemperatureValue {
    Number(Number),
    String(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ScalarValue {
    Number(Number),
    String(String),
}
