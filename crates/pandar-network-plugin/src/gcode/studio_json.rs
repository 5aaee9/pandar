use serde::{Deserialize, Deserializer, de::IgnoredAny};

use super::{
    PrintErrorAction, PrinterOperation, StudioOperationParse,
    operation::{AxisMovement, RequiredDeviceFeature},
    studio_axis::{StudioAxis, StudioDirection, StudioMoveMode},
};

#[derive(Deserialize)]
struct StudioMessage {
    system: Option<StudioSystem>,
    print: Option<StudioPrint>,
}

#[derive(Deserialize)]
struct StudioSystem {
    command: String,
    led_node: Option<String>,
    led_mode: Option<String>,
}

#[derive(Deserialize)]
struct StudioPrint {
    command: String,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    param: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    err: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    job_id: StudioFieldPresence,
    #[serde(default, deserialize_with = "deserialize_studio_field_presence")]
    sequence_id: StudioFieldPresence,
    extruder_index: Option<StudioU64>,
    target_temp: Option<StudioU64>,
    temp: Option<StudioU64>,
    ctt_val: Option<StudioU64>,
    ams_id: Option<StudioU64>,
    slot_id: Option<StudioU64>,
    target: Option<StudioU64>,
    extruder_id: Option<StudioU64>,
    axis: Option<StudioAxis>,
    dir: Option<StudioDirection>,
    mode: Option<StudioMoveMode>,
}

#[derive(Default)]
enum StudioFieldPresence {
    #[default]
    Absent,
    Present(StudioField),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StudioField {
    String(String),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Invalid(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StudioU64 {
    Number(u64),
    String(String),
}

fn deserialize_studio_field_presence<'de, D>(
    deserializer: D,
) -> Result<StudioFieldPresence, D::Error>
where
    D: Deserializer<'de>,
{
    StudioField::deserialize(deserializer).map(StudioFieldPresence::Present)
}

pub(super) fn parse_studio_json_operation(message: &str) -> StudioOperationParse {
    serde_json::from_str::<StudioMessage>(message)
        .map_or(StudioOperationParse::Unsupported, StudioMessage::operation)
}

impl StudioMessage {
    fn operation(self) -> StudioOperationParse {
        if let Some(system) = self.system {
            return system.operation().map_or(
                StudioOperationParse::Unsupported,
                StudioOperationParse::Operation,
            );
        }
        self.print
            .map_or(StudioOperationParse::Unsupported, StudioPrint::operation)
    }
}

impl StudioSystem {
    fn operation(self) -> Option<PrinterOperation> {
        if self.command != "ledctrl" {
            return None;
        }
        if !matches!(
            self.led_node.as_deref(),
            Some("chamber_light" | "chamber_light2")
        ) {
            return None;
        }

        let light_on = match self.led_mode.as_deref()? {
            "on" => true,
            "off" => false,
            _ => return None,
        };
        Some(PrinterOperation::SetChamberLight { light_on })
    }
}

impl StudioPrint {
    fn operation(self) -> StudioOperationParse {
        if let Some(action) = self.native_error_action()
            && self.is_native_candidate(action)
        {
            return self.native_error_operation(action).map_or(
                StudioOperationParse::InvalidNativeCandidate,
                StudioOperationParse::Operation,
            );
        }

        self.ordinary_operation()
            .filter(PrinterOperation::is_valid)
            .map_or(
                StudioOperationParse::Unsupported,
                StudioOperationParse::Operation,
            )
    }

    fn native_error_action(&self) -> Option<PrintErrorAction> {
        match self.command.as_str() {
            "resume" => Some(PrintErrorAction::Resume),
            "ignore" => Some(PrintErrorAction::Ignore),
            "stop" => Some(PrintErrorAction::Stop),
            _ => None,
        }
    }

    fn is_native_candidate(&self, action: PrintErrorAction) -> bool {
        action == PrintErrorAction::Ignore
            || self.err.is_present()
            || !self.param.is_absent_or_empty_string()
    }

    fn native_error_operation(&self, error_action: PrintErrorAction) -> Option<PrinterOperation> {
        if self.param.as_string()? != "reserve" {
            return None;
        }
        let print_error = self.err.as_string()?.parse::<u32>().ok()?;
        let sequence_id = self.sequence_id.as_string()?.parse::<u64>().ok()?;
        let operation = PrinterOperation::HandlePrintError {
            error_action,
            print_error,
            printer_job_id: self.job_id.as_string()?.to_owned(),
            sequence_id,
        };
        operation.is_valid().then_some(operation)
    }

    fn ordinary_operation(&self) -> Option<PrinterOperation> {
        match self.command.as_str() {
            "pause" => Some(PrinterOperation::Pause),
            "resume" => Some(PrinterOperation::Resume),
            "stop" => Some(PrinterOperation::Stop),
            "back_to_center" => Some(PrinterOperation::Home {
                axes: Some(Vec::new()),
                required_device_features: vec![RequiredDeviceFeature::BambuMqttHoming],
            }),
            "xyz_ctrl" => self.xyz_ctrl_operation(),
            "gcode_line" => super::parse_gcode_operation(self.param.as_string()?),
            "print_speed" => Some(PrinterOperation::SetPrintSpeed {
                speed_mode: self.param.as_u64()?,
            }),
            "select_extruder" => Some(PrinterOperation::SelectExtruder {
                extruder_id: field_u64(&self.extruder_index)?,
            }),
            "set_nozzle_temp" => Some(PrinterOperation::SetHotendTemperature {
                temperature_celsius: field_u64(&self.target_temp)?,
                wait: Some(false),
                extruder_id: Some(field_u64(&self.extruder_index)?),
            }),
            "set_bed_temp" => Some(PrinterOperation::SetBedTemperature {
                temperature_celsius: field_u64(&self.temp)?,
                wait: Some(false),
            }),
            "set_ctt" => Some(PrinterOperation::SetChamberTemperature {
                temperature_celsius: field_u64(&self.ctt_val)?,
                wait: Some(false),
            }),
            "ams_get_rfid" => Some(PrinterOperation::AmsRereadRfid {
                ams_id: field_u64(&self.ams_id)?,
                slot_id: field_u64(&self.slot_id)?,
            }),
            "ams_change_filament" => self.ams_change_filament_operation(),
            _ => None,
        }
    }

    fn xyz_ctrl_operation(&self) -> Option<PrinterOperation> {
        let axis = self.axis?.operation_axis();
        let delta_mm = self.dir?.sign() * self.mode?.distance_mm();
        Some(PrinterOperation::MoveAxes {
            movements: vec![AxisMovement { axis, delta_mm }],
            feedrate_mm_per_min: None,
            required_device_features: vec![RequiredDeviceFeature::BambuMqttAxisControl],
        })
    }

    fn ams_change_filament_operation(&self) -> Option<PrinterOperation> {
        let ams_id = field_u64(&self.ams_id)?;
        let slot_id = field_u64(&self.slot_id)?;
        let target = field_u64(&self.target)?;
        let extruder_id = optional_field_u64(&self.extruder_id);
        if target == 255 && slot_id == 255 {
            return Some(PrinterOperation::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id: None,
                external_id: None,
                extruder_id,
            });
        }

        Some(PrinterOperation::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id: Some(target),
            external_id: None,
            extruder_id,
        })
    }
}

impl StudioFieldPresence {
    fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    fn is_absent_or_empty_string(&self) -> bool {
        match self {
            Self::Absent => true,
            Self::Present(StudioField::String(value)) => value.is_empty(),
            Self::Present(_) => false,
        }
    }

    fn as_string(&self) -> Option<&str> {
        match self {
            Self::Present(StudioField::String(value)) => Some(value),
            _ => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Present(StudioField::Unsigned(value)) => Some(*value),
            Self::Present(StudioField::String(value)) => value.parse().ok(),
            Self::Present(StudioField::Signed(value)) => {
                let _ = value;
                None
            }
            Self::Present(StudioField::Float(value)) => {
                let _ = value;
                None
            }
            Self::Present(StudioField::Invalid(value)) => {
                let _ = value;
                None
            }
            Self::Absent => None,
        }
    }
}

impl StudioU64 {
    fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::String(value) => value.parse().ok(),
        }
    }
}

fn field_u64(value: &Option<StudioU64>) -> Option<u64> {
    value.as_ref().and_then(StudioU64::as_u64)
}

fn optional_field_u64(value: &Option<StudioU64>) -> Option<u64> {
    value.as_ref().and_then(StudioU64::as_u64)
}
