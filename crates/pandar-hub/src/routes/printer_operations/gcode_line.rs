use super::{PluginPrinterOperation, PrinterOperationRequest, invalid_printer_control};
use crate::{repositories::PrinterOperationKind, routes::ApiError};

pub(super) fn from_plugin_request(
    request: PrinterOperationRequest,
) -> Result<PluginPrinterOperation, ApiError> {
    if request.action != "gcode_line"
        || !request.param.is_some()
        || !request.required_device_features.is_missing()
        || !request.no_operation_fields()
        || !request.no_native_fields()
    {
        return Err(invalid_printer_control());
    }

    Ok(PluginPrinterOperation::Queued(
        PrinterOperationKind::GcodeLine {
            param: request.param.expect("checked above"),
        },
    ))
}
