use crate::{
    machine::PrinterOperation as MachinePrinterOperation, protocol::agent::v1::printer_operation,
};

const MIN_AMS_DRYING_TEMPERATURE_CELSIUS: u32 = 45;
const MAX_AMS_DRYING_TEMPERATURE_CELSIUS: u32 = 85;
const MIN_AMS_DRYING_DURATION_HOURS: u32 = 1;
const MAX_AMS_DRYING_DURATION_HOURS: u32 = 24;

pub(super) fn parse_ams_operation(
    operation: &printer_operation::Operation,
) -> anyhow::Result<MachinePrinterOperation> {
    match operation {
        printer_operation::Operation::AmsRereadRfid(operation) => {
            Ok(MachinePrinterOperation::AmsRereadRfid {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
            })
        }
        printer_operation::Operation::AmsLoadFilament(operation) => {
            Ok(MachinePrinterOperation::AmsLoadFilament {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
                global_tray_id: Some(operation.global_tray_id),
                external_id: (!operation.external_id.is_empty())
                    .then(|| operation.external_id.clone()),
                extruder_id: operation.extruder_id,
            })
        }
        printer_operation::Operation::AmsUnloadFilament(operation) => {
            Ok(MachinePrinterOperation::AmsUnloadFilament {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
                global_tray_id: Some(operation.global_tray_id),
                external_id: (!operation.external_id.is_empty())
                    .then(|| operation.external_id.clone()),
                extruder_id: operation.extruder_id,
            })
        }
        printer_operation::Operation::AmsStartDrying(operation) => {
            if !(MIN_AMS_DRYING_TEMPERATURE_CELSIUS..=MAX_AMS_DRYING_TEMPERATURE_CELSIUS)
                .contains(&operation.temperature_celsius)
            {
                anyhow::bail!(
                    "invalid printer operation drying temperature; expected {MIN_AMS_DRYING_TEMPERATURE_CELSIUS}..={MAX_AMS_DRYING_TEMPERATURE_CELSIUS}"
                );
            }
            if !(MIN_AMS_DRYING_DURATION_HOURS..=MAX_AMS_DRYING_DURATION_HOURS)
                .contains(&operation.duration_hours)
            {
                anyhow::bail!(
                    "invalid printer operation drying duration; expected {MIN_AMS_DRYING_DURATION_HOURS}..={MAX_AMS_DRYING_DURATION_HOURS}"
                );
            }
            Ok(MachinePrinterOperation::AmsStartDrying {
                ams_id: operation.ams_id,
                temperature_celsius: operation.temperature_celsius as u16,
                duration_hours: operation.duration_hours as u16,
                filament: operation.filament.clone(),
                rotate_tray: operation.rotate_tray,
            })
        }
        printer_operation::Operation::AmsStopDrying(operation) => {
            Ok(MachinePrinterOperation::AmsStopDrying {
                ams_id: operation.ams_id,
            })
        }
        _ => anyhow::bail!("not an AMS printer operation"),
    }
}

pub(super) fn refresh_materials_after_operation(operation: &MachinePrinterOperation) -> bool {
    matches!(
        operation,
        MachinePrinterOperation::AmsRereadRfid { .. }
            | MachinePrinterOperation::AmsLoadFilament { .. }
            | MachinePrinterOperation::AmsUnloadFilament { .. }
            | MachinePrinterOperation::AmsStartDrying { .. }
            | MachinePrinterOperation::AmsStopDrying { .. }
    )
}
