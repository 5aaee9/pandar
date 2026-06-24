use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::repositories::{RepositoryError, RepositoryResult};

const MAX_MOVE_DELTA_MM: f64 = 50.0;
const MIN_MOVE_FEEDRATE_MM_PER_MIN: u32 = 1;
const MAX_MOVE_FEEDRATE_MM_PER_MIN: u32 = 12_000;
const MAX_HOTEND_TEMPERATURE_CELSIUS: u16 = 300;

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
    SetPrintSpeed {
        speed_mode: u8,
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
    },
}

impl PrinterOperationKind {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::SetPrintSpeed { .. } => "set_print_speed",
            Self::Home { .. } => "home",
            Self::MoveAxes { .. } => "move_axes",
            Self::SetHotendTemperature { .. } => "set_hotend_temperature",
        }
    }
}

pub fn validate_printer_operation(operation: &PrinterOperationKind) -> RepositoryResult<()> {
    match operation {
        PrinterOperationKind::Pause | PrinterOperationKind::Resume | PrinterOperationKind::Stop => {
            Ok(())
        }
        PrinterOperationKind::SetPrintSpeed { speed_mode } if (1..=4).contains(speed_mode) => {
            Ok(())
        }
        PrinterOperationKind::SetPrintSpeed { .. } => Err(RepositoryError::InvalidPrinterControl),
        PrinterOperationKind::Home { .. } => Ok(()),
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
        } => validate_move_axes(movements, *feedrate_mm_per_min),
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius,
            ..
        } if *temperature_celsius <= MAX_HOTEND_TEMPERATURE_CELSIUS => Ok(()),
        PrinterOperationKind::SetHotendTemperature { .. } => {
            Err(RepositoryError::InvalidPrinterControl)
        }
    }
}

pub fn operation_audit_metadata(
    agent_id: String,
    serial_number: String,
    operation: &PrinterOperationKind,
) -> Value {
    let mut metadata = serde_json::Map::from_iter([
        ("agent_id".to_owned(), json!(agent_id)),
        ("serial_number".to_owned(), json!(serial_number)),
        ("action".to_owned(), json!(operation.action())),
    ]);

    match operation {
        PrinterOperationKind::SetPrintSpeed { speed_mode } => {
            metadata.insert("speed_mode".to_owned(), json!(speed_mode));
        }
        PrinterOperationKind::Home { axes } => {
            metadata.insert("axes".to_owned(), json!(axis_names(axes)));
        }
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
        } => {
            metadata.insert(
                "movements".to_owned(),
                json!(
                    movements
                        .iter()
                        .map(|movement| json!({
                            "axis": movement.axis.as_str(),
                            "delta_mm": movement.delta_mm,
                        }))
                        .collect::<Vec<_>>()
                ),
            );
            metadata.insert("feedrate_mm_per_min".to_owned(), json!(feedrate_mm_per_min));
        }
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius,
            wait,
        } => {
            metadata.insert("temperature_celsius".to_owned(), json!(temperature_celsius));
            metadata.insert("wait".to_owned(), json!(wait));
        }
        PrinterOperationKind::Pause | PrinterOperationKind::Resume | PrinterOperationKind::Stop => {
        }
    }

    Value::Object(metadata)
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

fn axis_names(axes: &[PrinterAxis]) -> Vec<&'static str> {
    axes.iter().map(|axis| axis.as_str()).collect()
}
