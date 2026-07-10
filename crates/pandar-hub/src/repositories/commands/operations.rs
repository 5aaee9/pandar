use serde::{Deserialize, Serialize};

mod audit;

pub use audit::operation_audit_metadata;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterOperationPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

impl PrinterAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterAxisMovement {
    pub axis: PrinterAxis,
    pub delta_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrinterOperationKind {
    Pause,
    Resume,
    Stop,
    HandlePrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    ToggleLight,
    SetChamberLight {
        on: bool,
    },
    SetPrintSpeed {
        speed_mode: u8,
    },
    SelectExtruder {
        extruder_id: u32,
    },
    Home {
        #[serde(default)]
        axes: Vec<PrinterAxis>,
    },
    MoveAxes {
        movements: Vec<PrinterAxisMovement>,
        #[serde(default)]
        feedrate_mm_per_min: Option<u32>,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u32>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    AmsRereadRfid {
        ams_id: u32,
        slot_id: u32,
    },
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsUnloadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
}

impl PrinterOperationKind {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::HandlePrintError { .. } => "handle_print_error",
            Self::ToggleLight => "toggle_light",
            Self::SetChamberLight { .. } => "set_chamber_light",
            Self::SetPrintSpeed { .. } => "set_print_speed",
            Self::SelectExtruder { .. } => "select_extruder",
            Self::Home { .. } => "home",
            Self::MoveAxes { .. } => "move_axes",
            Self::SetHotendTemperature { .. } => "set_hotend_temperature",
            Self::SetBedTemperature { .. } => "set_bed_temperature",
            Self::SetChamberTemperature { .. } => "set_chamber_temperature",
            Self::AmsRereadRfid { .. } => "ams_reread_rfid",
            Self::AmsLoadFilament { .. } => "ams_load_filament",
            Self::AmsUnloadFilament { .. } => "ams_unload_filament",
        }
    }
}

pub fn validate_printer_operation(operation: &PrinterOperationKind) -> RepositoryResult<()> {
    match operation {
        PrinterOperationKind::Pause
        | PrinterOperationKind::Resume
        | PrinterOperationKind::Stop
        | PrinterOperationKind::ToggleLight
        | PrinterOperationKind::SetChamberLight { .. } => Ok(()),
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
        PrinterOperationKind::Home { .. } => Ok(()),
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
        } => validate_move_axes(movements, *feedrate_mm_per_min),
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
