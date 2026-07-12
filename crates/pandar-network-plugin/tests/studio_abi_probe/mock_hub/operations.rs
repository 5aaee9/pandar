use serde::Deserialize;

use crate::support::request_body;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum TestOperation {
    Home {
        axes: Vec<String>,
    },
    SetChamberLight {
        light_on: bool,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: u8,
    },
    HandlePrintError {
        error_action: TestPrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum TestPrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

pub(super) fn assert_operation_body_eq(request: &str, expected: TestOperation) {
    let actual: TestOperation = serde_json::from_str(request_body(request)).unwrap();
    assert_eq!(actual, expected);
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AxisFeatureOperation {
    Home {
        axes: Vec<String>,
        #[serde(default)]
        required_device_features: Option<Vec<String>>,
    },
    MoveAxes {
        movements: Vec<AxisFeatureMovement>,
        #[serde(default)]
        feedrate_mm_per_min: Option<u32>,
        #[serde(default)]
        required_device_features: Option<Vec<String>>,
    },
    GcodeLine {
        param: String,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct AxisFeatureMovement {
    axis: String,
    delta_mm: f64,
}

impl AxisFeatureOperation {
    pub(super) fn modern_home() -> Self {
        Self::Home {
            axes: Vec::new(),
            required_device_features: Some(vec!["bambu_mqtt_homing".to_owned()]),
        }
    }

    pub(super) fn modern_move(axis: &str, delta_mm: f64) -> Self {
        Self::MoveAxes {
            movements: vec![AxisFeatureMovement {
                axis: axis.to_owned(),
                delta_mm,
            }],
            feedrate_mm_per_min: None,
            required_device_features: Some(vec!["bambu_mqtt_axis_control".to_owned()]),
        }
    }

    pub(super) fn legacy_home() -> Self {
        Self::Home {
            axes: vec!["x".to_owned()],
            required_device_features: None,
        }
    }

    pub(super) fn legacy_move(axis: &str, delta_mm: f64, feedrate: u32) -> Self {
        Self::MoveAxes {
            movements: vec![AxisFeatureMovement {
                axis: axis.to_owned(),
                delta_mm,
            }],
            feedrate_mm_per_min: Some(feedrate),
            required_device_features: None,
        }
    }

    pub(super) fn gcode_line(param: &str) -> Self {
        Self::GcodeLine {
            param: param.to_owned(),
        }
    }
}

pub(super) fn assert_axis_feature_operation_body_eq(request: &str, expected: AxisFeatureOperation) {
    let body = request_body(request);
    let required_device_features_present = match &expected {
        AxisFeatureOperation::Home {
            required_device_features,
            ..
        }
        | AxisFeatureOperation::MoveAxes {
            required_device_features,
            ..
        } => required_device_features.is_some(),
        AxisFeatureOperation::GcodeLine { .. } => false,
    };
    assert!(
        required_device_feature_presence_matches(body, required_device_features_present),
        "required_device_features presence did not match semantics: {body}"
    );
    let actual: AxisFeatureOperation = serde_json::from_str(body).unwrap();
    assert_eq!(actual, expected);
    if matches!(
        expected,
        AxisFeatureOperation::Home { .. } | AxisFeatureOperation::MoveAxes { .. }
    ) {
        for raw_transport in ["G28", "M211", "xyz_ctrl", "back_to_center"] {
            assert!(
                !body.contains(raw_transport),
                "operation request leaked raw Studio transport {raw_transport}: {body}"
            );
        }
    }
}

pub(crate) fn required_device_feature_presence_matches(body: &str, expected_present: bool) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|object| object.contains_key("required_device_features") == expected_present)
}
