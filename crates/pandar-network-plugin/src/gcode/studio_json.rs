use serde::Deserialize;

use super::PrinterOperation;

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
    param: Option<StudioU64>,
    extruder_index: Option<StudioU64>,
    target_temp: Option<StudioU64>,
    temp: Option<StudioU64>,
    ctt_val: Option<StudioU64>,
    ams_id: Option<StudioU64>,
    slot_id: Option<StudioU64>,
    target: Option<StudioU64>,
    extruder_id: Option<StudioU64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StudioU64 {
    Number(u64),
    String(String),
}

pub(super) fn parse_studio_json_operation(message: &str) -> Option<PrinterOperation> {
    serde_json::from_str::<StudioMessage>(message)
        .ok()?
        .operation()
}

impl StudioMessage {
    fn operation(self) -> Option<PrinterOperation> {
        if let Some(system) = self.system {
            return system.operation();
        }
        self.print?.operation()
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
    fn operation(self) -> Option<PrinterOperation> {
        let operation = match self.command.as_str() {
            "pause" => PrinterOperation::Pause,
            "resume" => PrinterOperation::Resume,
            "stop" => PrinterOperation::Stop,
            "print_speed" => PrinterOperation::SetPrintSpeed {
                speed_mode: field_u64(&self.param)?,
            },
            "select_extruder" => PrinterOperation::SelectExtruder {
                extruder_id: field_u64(&self.extruder_index)?,
            },
            "set_nozzle_temp" => PrinterOperation::SetHotendTemperature {
                temperature_celsius: field_u64(&self.target_temp)?,
                wait: Some(false),
                extruder_id: Some(field_u64(&self.extruder_index)?),
            },
            "set_bed_temp" => PrinterOperation::SetBedTemperature {
                temperature_celsius: field_u64(&self.temp)?,
                wait: Some(false),
            },
            "set_ctt" => PrinterOperation::SetChamberTemperature {
                temperature_celsius: field_u64(&self.ctt_val)?,
                wait: Some(false),
            },
            "ams_get_rfid" => PrinterOperation::AmsRereadRfid {
                ams_id: field_u64(&self.ams_id)?,
                slot_id: field_u64(&self.slot_id)?,
            },
            "ams_change_filament" => self.ams_change_filament_operation()?,
            _ => return None,
        };
        operation.is_valid().then_some(operation)
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
