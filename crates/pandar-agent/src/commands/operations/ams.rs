use anyhow::Context;
use pandar_core::PrinterOperation;
use pandar_protocol::agent::v1::printer_operation;

pub(super) fn parse_ams_operation(
    operation: &printer_operation::Operation,
) -> anyhow::Result<PrinterOperation> {
    match operation {
        printer_operation::Operation::AmsRereadRfid(operation) => {
            Ok(PrinterOperation::AmsRereadRfid {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
            })
        }
        printer_operation::Operation::AmsLoadFilament(operation) => {
            Ok(PrinterOperation::AmsLoadFilament {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
                global_tray_id: Some(operation.global_tray_id),
                external_id: (!operation.external_id.is_empty())
                    .then(|| operation.external_id.clone()),
                extruder_id: operation.extruder_id,
            })
        }
        printer_operation::Operation::AmsUnloadFilament(operation) => {
            Ok(PrinterOperation::AmsUnloadFilament {
                ams_id: operation.ams_id,
                slot_id: operation.slot_id,
                global_tray_id: Some(operation.global_tray_id),
                external_id: (!operation.external_id.is_empty())
                    .then(|| operation.external_id.clone()),
                extruder_id: operation.extruder_id,
            })
        }
        printer_operation::Operation::AmsStartDrying(operation) => {
            Ok(PrinterOperation::AmsStartDrying {
                ams_id: operation.ams_id,
                temperature_celsius: u16::try_from(operation.temperature_celsius)
                    .context("printer operation drying temperature exceeds uint16")?,
                duration_hours: u16::try_from(operation.duration_hours)
                    .context("printer operation drying duration exceeds uint16")?,
                filament: operation.filament.clone(),
                rotate_tray: operation.rotate_tray,
            })
        }
        printer_operation::Operation::AmsStopDrying(operation) => {
            Ok(PrinterOperation::AmsStopDrying {
                ams_id: operation.ams_id,
            })
        }
        _ => anyhow::bail!("not an AMS printer operation"),
    }
}

pub(super) fn refresh_materials_after_operation(operation: &PrinterOperation) -> bool {
    matches!(
        operation,
        PrinterOperation::AmsRereadRfid { .. }
            | PrinterOperation::AmsLoadFilament { .. }
            | PrinterOperation::AmsUnloadFilament { .. }
            | PrinterOperation::AmsStartDrying { .. }
            | PrinterOperation::AmsStopDrying { .. }
    )
}
