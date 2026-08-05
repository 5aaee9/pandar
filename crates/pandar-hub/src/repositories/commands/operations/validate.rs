use super::{PrinterAxisMovement, PrinterOperationKind};
use crate::repositories::{RepositoryError, RepositoryResult};

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

fn valid_rack_nozzle_id(id: u32) -> bool {
    (16..=21).contains(&id) || id == RACK_NOZZLE_ID_ALL
}

pub fn validate_printer_operation(operation: &PrinterOperationKind) -> RepositoryResult<()> {
    match operation {
        PrinterOperationKind::GetAutoNozzleMapping { request } if request.is_valid() => Ok(()),
        PrinterOperationKind::GetAutoNozzleMapping { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::NozzleHolderCtrl { action } if *action <= MAX_HOLDER_CTRL_ACTION => {
            Ok(())
        }
        PrinterOperationKind::NozzleHolderCtrl { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::NozzleInfoConfirm { id }
        | PrinterOperationKind::HolderNozzleRefresh { id }
            if valid_rack_nozzle_id(*id) =>
        {
            Ok(())
        }
        PrinterOperationKind::NozzleInfoConfirm { .. }
        | PrinterOperationKind::HolderNozzleRefresh { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::Pause {}
        | PrinterOperationKind::Resume {}
        | PrinterOperationKind::Stop {}
        | PrinterOperationKind::ToggleLight {}
        | PrinterOperationKind::SetChamberLight { .. }
        | PrinterOperationKind::GcodeLine { .. } => Ok(()),
        PrinterOperationKind::HandlePrintError { print_error, .. }
            if (1..=i32::MAX as u32).contains(print_error) =>
        {
            Ok(())
        }
        PrinterOperationKind::HandlePrintError { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::SetPrintSpeed { speed_mode } if (1..=4).contains(speed_mode) => {
            Ok(())
        }
        PrinterOperationKind::SetPrintSpeed { .. } => Err(RepositoryError::InvalidPrinterControl),
        PrinterOperationKind::SelectExtruder { extruder_id } if *extruder_id <= MAX_EXTRUDER_ID => {
            Ok(())
        }
        PrinterOperationKind::SelectExtruder { .. } => Err(RepositoryError::InvalidPrinterControl),
        PrinterOperationKind::Home { .. } if operation.has_valid_required_device_features() => {
            Ok(())
        }
        PrinterOperationKind::Home { .. } => Err(RepositoryError::InvalidPrinterControl),
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
            ..
        } => {
            validate_move_axes(movements, *feedrate_mm_per_min)?;
            if operation.has_valid_required_device_features() {
                Ok(())
            } else {
                Err(RepositoryError::InvalidPrinterControl)
            }
        }
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius,
            extruder_id,
            ..
        } if *temperature_celsius <= MAX_HOTEND_TEMPERATURE_CELSIUS
            && extruder_id.is_none_or(|value| value <= MAX_EXTRUDER_ID) =>
        {
            Ok(())
        }
        PrinterOperationKind::SetHotendTemperature { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::SetBedTemperature {
            temperature_celsius,
            ..
        } if *temperature_celsius <= MAX_BED_TEMPERATURE_CELSIUS => Ok(()),
        PrinterOperationKind::SetBedTemperature { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::SetChamberTemperature {
            temperature_celsius,
            ..
        } if *temperature_celsius <= MAX_CHAMBER_TEMPERATURE_CELSIUS => Ok(()),
        PrinterOperationKind::SetChamberTemperature { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::AmsRereadRfid { ams_id, slot_id }
            if *ams_id <= MAX_AMS_ID && *slot_id <= MAX_AMS_SLOT_ID =>
        {
            Ok(())
        }
        PrinterOperationKind::AmsLoadFilament {
            ams_id,
            slot_id,
            extruder_id,
            ..
        }
        | PrinterOperationKind::AmsUnloadFilament {
            ams_id,
            slot_id,
            extruder_id,
            ..
        } if *ams_id <= MAX_AMS_ID
            && *slot_id <= MAX_AMS_SLOT_ID
            && extruder_id.is_none_or(|value| value <= MAX_EXTRUDER_ID) =>
        {
            Ok(())
        }
        PrinterOperationKind::AmsRereadRfid { .. }
        | PrinterOperationKind::AmsLoadFilament { .. }
        | PrinterOperationKind::AmsUnloadFilament { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
        PrinterOperationKind::AmsStartDrying {
            ams_id,
            temperature_celsius,
            duration_hours,
            filament,
            ..
        } if *ams_id <= MAX_AMS_ID
            && (MIN_AMS_DRYING_TEMPERATURE_CELSIUS..=MAX_AMS_DRYING_TEMPERATURE_CELSIUS)
                .contains(temperature_celsius)
            && (MIN_AMS_DRYING_DURATION_HOURS..=MAX_AMS_DRYING_DURATION_HOURS)
                .contains(duration_hours)
            && !filament.trim().is_empty() =>
        {
            Ok(())
        }
        PrinterOperationKind::AmsStartDrying { .. } => Err(RepositoryError::InvalidPrinterControl),
        PrinterOperationKind::AmsStopDrying { ams_id } if *ams_id <= MAX_AMS_ID => Ok(()),
        PrinterOperationKind::AmsStopDrying { .. } => Err(RepositoryError::InvalidPrinterControl),
    }
}

fn validate_move_axes(
    movements: &[PrinterAxisMovement],
    feedrate_mm_per_min: Option<u32>,
) -> RepositoryResult<()> {
    let mut seen_axes = Vec::new();
    if movements.is_empty()
        || movements.iter().any(|movement| {
            let invalid = movement.delta_mm == 0.0
                || movement.delta_mm.abs() > MAX_MOVE_DELTA_MM
                || seen_axes.contains(&movement.axis);
            seen_axes.push(movement.axis);
            invalid
        })
    {
        return Err(RepositoryError::InvalidPrinterControl);
    }

    if let Some(feedrate) = feedrate_mm_per_min
        && !(MIN_MOVE_FEEDRATE_MM_PER_MIN..=MAX_MOVE_FEEDRATE_MM_PER_MIN).contains(&feedrate)
    {
        return Err(RepositoryError::InvalidPrinterControl);
    }

    Ok(())
}
