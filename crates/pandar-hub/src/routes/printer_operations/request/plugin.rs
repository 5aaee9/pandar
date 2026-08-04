use super::{
    super::{device_features, gcode_line, invalid_printer_control},
    PluginPrinterOperation, PrinterOperationRequest, TenantPrinterOperation,
};
use crate::{repositories::PrinterOperationKind, routes::ApiError};

impl PrinterOperationRequest {
    pub(in crate::routes) fn into_plugin_operation(
        self,
    ) -> Result<PluginPrinterOperation, ApiError> {
        if self.action == "gcode_line" {
            return gcode_line::from_plugin_request(self);
        }
        if !self.param.is_missing() {
            return Err(invalid_printer_control());
        }
        if self.action != "handle_print_error" {
            return self
                .into_tenant_operation()
                .and_then(|operation| match operation {
                    TenantPrinterOperation::Queued(operation) => {
                        Ok(PluginPrinterOperation::Queued(operation))
                    }
                    TenantPrinterOperation::HandlePrintError { .. } => {
                        Err(invalid_printer_control())
                    }
                });
        }
        device_features::from_request(&self)?;
        if !self.no_operation_fields() || !self.error_generation.is_missing() {
            return Err(invalid_printer_control());
        }
        let (Some(error_action), Some(print_error), Some(printer_job_id), Some(sequence_id)) = (
            self.error_action.into_option(),
            self.print_error.into_option(),
            self.printer_job_id.into_option(),
            self.sequence_id.into_option(),
        ) else {
            return Err(invalid_printer_control());
        };
        if sequence_id == 0 {
            return Err(invalid_printer_control());
        }
        let operation = PrinterOperationKind::HandlePrintError {
            error_action,
            print_error,
            printer_job_id,
            sequence_id,
        };
        if !(1..=i32::MAX as u32).contains(&print_error) {
            return Err(invalid_printer_control());
        }
        Ok(PluginPrinterOperation::Live(operation))
    }
}
