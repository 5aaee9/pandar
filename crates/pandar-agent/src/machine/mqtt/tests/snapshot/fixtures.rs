use serde::Serialize;
use serde_json::Value;

#[derive(Default, Serialize)]
pub(super) struct SnapshotReportFixture {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<ScalarFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) print: Option<SnapshotPrintFixture>,
}

#[derive(Default, Serialize)]
pub(super) struct SnapshotPrintFixture {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) fun: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) gcode_state: Option<ScalarFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<ScalarFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_temper: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_target_temper: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_temper2: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_target_temper2: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bed_temper: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bed_target_temper: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) chamber_temper: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) lights_report: Vec<LightReportFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) device: Option<DeviceFixture>,
}

#[derive(Default, Serialize)]
pub(super) struct DeviceFixture {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bed_temp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ctc: Option<CtcFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extruder: Option<ExtruderFixture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle: Option<NozzleFixture>,
}

#[derive(Serialize)]
pub(super) struct CtcFixture {
    pub(super) state: u64,
    pub(super) info: TemperatureInfoFixture,
}

#[derive(Serialize)]
pub(super) struct TemperatureInfoFixture {
    pub(super) temp: i64,
}

#[derive(Serialize)]
pub(super) struct ExtruderFixture {
    pub(super) state: u64,
    pub(super) info: Vec<ExtruderInfoFixture>,
}

#[derive(Serialize)]
pub(super) struct ExtruderInfoFixture {
    pub(super) id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) info: Option<u64>,
    pub(super) temp: i64,
}

#[derive(Serialize)]
pub(super) struct NozzleFixture {
    pub(super) exist: u64,
    pub(super) info: Vec<NozzleInfoFixture>,
}

#[derive(Serialize)]
pub(super) struct NozzleInfoFixture {
    pub(super) id: u64,
    pub(super) diameter: f64,
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) stat: u64,
}

#[derive(Serialize)]
pub(super) struct LightReportFixture {
    pub(super) node: &'static str,
    pub(super) mode: &'static str,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ScalarFixture {
    Text(&'static str),
    Number(i64),
}

pub(super) fn report_with_print(print: SnapshotPrintFixture) -> Value {
    value(SnapshotReportFixture {
        print: Some(print),
        ..Default::default()
    })
}

pub(super) fn extruder_temperatures(left: i64, right: i64) -> Vec<ExtruderInfoFixture> {
    vec![
        ExtruderInfoFixture {
            id: 0,
            info: None,
            temp: left,
        },
        ExtruderInfoFixture {
            id: 1,
            info: None,
            temp: right,
        },
    ]
}

pub(super) fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
