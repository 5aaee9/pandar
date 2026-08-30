use thiserror::Error;

use super::{PrinterAxis, PrinterAxisMovement, PrinterOperation};
use crate::RequiredDeviceFeature;

const MAX_MOVE_DELTA_MM: f64 = 50.0;
const MIN_MOVE_FEEDRATE_MM_PER_MIN: u32 = 1;
const MAX_MOVE_FEEDRATE_MM_PER_MIN: u32 = 12_000;
const MAX_HOTEND_TEMPERATURE_CELSIUS: u16 = 300;
const MAX_BED_TEMPERATURE_CELSIUS: u16 = 120;
const MAX_CHAMBER_TEMPERATURE_CELSIUS: u16 = 70;
const MAX_AMS_ID: u32 = 255;
const MAX_AMS_SLOT_ID: u32 = 255;
const MAX_EXTRUDER_ID: u32 = 1;
const MAX_HOLDER_CTRL_ACTION: u32 = 2;
const RACK_NOZZLE_ID_ALL: u32 = 0xff;
const MIN_AMS_DRYING_TEMPERATURE_CELSIUS: u16 = 45;
const MAX_AMS_DRYING_TEMPERATURE_CELSIUS: u16 = 85;
const MIN_AMS_DRYING_DURATION_HOURS: u16 = 1;
const MAX_AMS_DRYING_DURATION_HOURS: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct PrinterOperationValidationError {
    message: &'static str,
}

impl PrinterOperation {
    pub fn validate(&self) -> Result<(), PrinterOperationValidationError> {
        match self {
            Self::GetAutoNozzleMapping { request } if !request.is_valid() => {
                invalid("invalid H2C auto nozzle mapping request")
            }
            Self::NozzleHolderCtrl { action } if *action > MAX_HOLDER_CTRL_ACTION => {
                invalid("invalid H2C nozzle_holder_ctrl action; expected 0..=2")
            }
            Self::NozzleInfoConfirm { id } if !valid_rack_nozzle_id(*id) => {
                invalid("invalid H2C nozzle_info_confirm id; expected 16..=21 or 255")
            }
            Self::HolderNozzleRefresh { id } if !valid_rack_nozzle_id(*id) => {
                invalid("invalid H2C holder_nozzle_refresh id; expected 16..=21 or 255")
            }
            Self::HandlePrintError { print_error, .. }
                if !(1..=i32::MAX as u32).contains(print_error) =>
            {
                invalid("invalid printer operation print_error; expected positive int32")
            }
            Self::SetPrintSpeed { speed_mode } if !(1..=4).contains(speed_mode) => {
                invalid("invalid printer operation speed_mode; expected 1..=4")
            }
            Self::SetFanSpeed {
                fan_index,
                speed_percent,
                ..
            } if !(1..=3).contains(fan_index) || *speed_percent > 100 => {
                invalid("invalid printer operation fan speed")
            }
            Self::SelectExtruder { extruder_id } if *extruder_id > MAX_EXTRUDER_ID => {
                invalid("invalid printer operation extruder_id; expected 0..=1")
            }
            Self::Home {
                axes,
                required_device_features,
            } if !valid_home_features(axes, required_device_features) => {
                invalid("required device feature does not match home semantics")
            }
            Self::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            } => {
                validate_move_axes(movements, *feedrate_mm_per_min)?;
                if valid_move_features(movements, *feedrate_mm_per_min, required_device_features) {
                    Ok(())
                } else {
                    invalid("required device feature does not match axis movement semantics")
                }
            }
            Self::SetHotendTemperature {
                temperature_celsius,
                extruder_id,
                ..
            } if *temperature_celsius > MAX_HOTEND_TEMPERATURE_CELSIUS
                || extruder_id.is_some_and(|value| value > MAX_EXTRUDER_ID) =>
            {
                invalid("invalid printer operation hotend temperature or extruder id")
            }
            Self::SetBedTemperature {
                temperature_celsius,
                ..
            } if *temperature_celsius > MAX_BED_TEMPERATURE_CELSIUS => {
                invalid("invalid printer operation bed target")
            }
            Self::SetChamberTemperature {
                temperature_celsius,
                ..
            } if *temperature_celsius > MAX_CHAMBER_TEMPERATURE_CELSIUS => {
                invalid("invalid printer operation chamber target")
            }
            Self::AmsRereadRfid { ams_id, slot_id } if !valid_ams_slot(*ams_id, *slot_id) => {
                invalid("invalid AMS slot")
            }
            Self::AmsLoadFilament {
                ams_id,
                slot_id,
                extruder_id,
                ..
            }
            | Self::AmsUnloadFilament {
                ams_id,
                slot_id,
                extruder_id,
                ..
            } if !valid_ams_slot(*ams_id, *slot_id)
                || extruder_id.is_some_and(|value| value > MAX_EXTRUDER_ID) =>
            {
                invalid("invalid AMS filament operation")
            }
            Self::AmsStartDrying { ams_id, .. } if *ams_id > MAX_AMS_ID => {
                invalid("invalid AMS id")
            }
            Self::AmsStartDrying {
                temperature_celsius,
                ..
            } if !(MIN_AMS_DRYING_TEMPERATURE_CELSIUS..=MAX_AMS_DRYING_TEMPERATURE_CELSIUS)
                .contains(temperature_celsius) =>
            {
                invalid("invalid printer operation drying temperature; expected 45..=85")
            }
            Self::AmsStartDrying { duration_hours, .. }
                if !(MIN_AMS_DRYING_DURATION_HOURS..=MAX_AMS_DRYING_DURATION_HOURS)
                    .contains(duration_hours) =>
            {
                invalid("invalid printer operation drying duration; expected 1..=24")
            }
            Self::AmsStartDrying { filament, .. } if filament.trim().is_empty() => {
                invalid("invalid printer operation drying filament")
            }
            Self::AmsStopDrying { ams_id } if *ams_id > MAX_AMS_ID => invalid("invalid AMS id"),
            _ => Ok(()),
        }
    }
}

fn validate_move_axes(
    movements: &[PrinterAxisMovement],
    feedrate_mm_per_min: Option<u32>,
) -> Result<(), PrinterOperationValidationError> {
    if movements.is_empty() {
        return invalid("printer operation move_axes requires at least one axis");
    }
    let mut seen_axes = Vec::new();
    for movement in movements {
        if !movement.delta_mm.is_finite()
            || movement.delta_mm == 0.0
            || movement.delta_mm.abs() > MAX_MOVE_DELTA_MM
        {
            return invalid(
                "invalid printer operation move_axes delta_mm; expected finite nonzero value within 50mm",
            );
        }
        if seen_axes.contains(&movement.axis) {
            return invalid("printer operation move_axes contains duplicate axis");
        }
        seen_axes.push(movement.axis);
    }
    if feedrate_mm_per_min.is_some_and(|feedrate| {
        !(MIN_MOVE_FEEDRATE_MM_PER_MIN..=MAX_MOVE_FEEDRATE_MM_PER_MIN).contains(&feedrate)
    }) {
        return invalid("invalid printer operation move_axes feedrate; expected 1..=12000");
    }
    Ok(())
}

fn valid_home_features(axes: &[PrinterAxis], required: &[RequiredDeviceFeature]) -> bool {
    required.is_empty() || (axes.is_empty() && required == [RequiredDeviceFeature::BambuMqttHoming])
}

fn valid_move_features(
    movements: &[PrinterAxisMovement],
    feedrate: Option<u32>,
    required: &[RequiredDeviceFeature],
) -> bool {
    required.is_empty()
        || (movements.len() == 1
            && feedrate.is_none()
            && matches!(movements[0].delta_mm.abs(), 1.0 | 10.0)
            && required == [RequiredDeviceFeature::BambuMqttAxisControl])
}

fn valid_ams_slot(ams_id: u32, slot_id: u32) -> bool {
    ams_id <= MAX_AMS_ID && slot_id <= MAX_AMS_SLOT_ID
}

fn valid_rack_nozzle_id(id: u32) -> bool {
    (16..=21).contains(&id) || id == RACK_NOZZLE_ID_ALL
}

fn invalid<T>(message: &'static str) -> Result<T, PrinterOperationValidationError> {
    Err(PrinterOperationValidationError { message })
}
