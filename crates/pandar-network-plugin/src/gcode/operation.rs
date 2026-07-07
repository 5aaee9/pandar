use serde::{Deserialize, Serialize};

const MAX_EXTRUDER_ID: u64 = 1;
const MAX_HOTEND_TEMPERATURE_CELSIUS: u64 = 300;
const MAX_BED_TEMPERATURE_CELSIUS: u64 = 120;
const MAX_CHAMBER_TEMPERATURE_CELSIUS: u64 = 70;
const MAX_AMS_ID: u64 = 255;
const MAX_AMS_SLOT_ID: u64 = 255;
const MAX_U32: u64 = u32::MAX as u64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PrinterOperation {
    Pause,
    Resume,
    Stop,
    ToggleLight,
    SetChamberLight {
        light_on: bool,
    },
    SetPrintSpeed {
        speed_mode: u64,
    },
    SelectExtruder {
        extruder_id: u64,
    },
    Home {
        #[serde(skip_serializing_if = "Option::is_none")]
        axes: Option<Vec<Axis>>,
    },
    MoveAxes {
        movements: Vec<AxisMovement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        feedrate_mm_per_min: Option<u64>,
    },
    SetHotendTemperature {
        temperature_celsius: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extruder_id: Option<u64>,
    },
    SetBedTemperature {
        temperature_celsius: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
    },
    SetChamberTemperature {
        temperature_celsius: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
    },
    AmsRereadRfid {
        ams_id: u64,
        slot_id: u64,
    },
    AmsLoadFilament {
        ams_id: u64,
        slot_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        global_tray_id: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        external_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extruder_id: Option<u64>,
    },
    AmsUnloadFilament {
        ams_id: u64,
        slot_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        global_tray_id: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        external_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extruder_id: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(crate) struct AxisMovement {
    pub(crate) axis: Axis,
    pub(crate) delta_mm: f64,
}

impl PrinterOperation {
    pub(crate) fn from_json(body: &str) -> Option<Self> {
        serde_json::from_str::<Self>(body)
            .ok()
            .filter(Self::is_valid)
    }

    pub(super) fn is_valid(&self) -> bool {
        match self {
            Self::Pause | Self::Resume | Self::Stop | Self::ToggleLight => true,
            Self::SetChamberLight { .. } => true,
            Self::SetPrintSpeed { speed_mode } => (1..=4).contains(speed_mode),
            Self::SelectExtruder { extruder_id } => in_range(*extruder_id, 0, MAX_EXTRUDER_ID),
            Self::Home { .. } => true,
            Self::MoveAxes {
                movements,
                feedrate_mm_per_min,
            } => {
                !movements.is_empty()
                    && movements.iter().all(AxisMovement::is_valid)
                    && feedrate_mm_per_min.is_none_or(|feedrate| (1..=12_000).contains(&feedrate))
            }
            Self::SetHotendTemperature {
                temperature_celsius,
                extruder_id,
                ..
            } => {
                in_range(*temperature_celsius, 0, MAX_HOTEND_TEMPERATURE_CELSIUS)
                    && extruder_id
                        .is_none_or(|extruder_id| in_range(extruder_id, 0, MAX_EXTRUDER_ID))
            }
            Self::SetBedTemperature {
                temperature_celsius,
                ..
            } => in_range(*temperature_celsius, 0, MAX_BED_TEMPERATURE_CELSIUS),
            Self::SetChamberTemperature {
                temperature_celsius,
                ..
            } => in_range(*temperature_celsius, 0, MAX_CHAMBER_TEMPERATURE_CELSIUS),
            Self::AmsRereadRfid { ams_id, slot_id } => valid_ams_slot(*ams_id, *slot_id),
            Self::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                extruder_id,
                ..
            }
            | Self::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                extruder_id,
                ..
            } => {
                valid_ams_slot(*ams_id, *slot_id)
                    && global_tray_id.is_none_or(|value| in_range(value, 0, MAX_U32))
                    && extruder_id.is_none_or(|value| in_range(value, 0, MAX_EXTRUDER_ID))
            }
        }
    }
}

impl AxisMovement {
    fn is_valid(&self) -> bool {
        self.delta_mm.is_finite() && self.delta_mm != 0.0 && self.delta_mm.abs() <= 50.0
    }
}

fn valid_ams_slot(ams_id: u64, slot_id: u64) -> bool {
    in_range(ams_id, 0, MAX_AMS_ID) && in_range(slot_id, 0, MAX_AMS_SLOT_ID)
}

fn in_range(value: u64, min: u64, max: u64) -> bool {
    (min..=max).contains(&value)
}
