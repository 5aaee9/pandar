use super::{PrinterOperationRequest, invalid_printer_control, request_field::RequestField};
use crate::{grpc::commands::RequiredDeviceFeature, routes::ApiError};

pub(super) fn from_request(
    request: &PrinterOperationRequest,
) -> Result<Vec<RequiredDeviceFeature>, ApiError> {
    let required = match &request.required_device_features {
        RequestField::Missing => return Ok(Vec::new()),
        RequestField::Present(Some(required)) => required,
        RequestField::Present(None) => return Err(invalid_printer_control()),
    };
    if required.is_empty() {
        return Ok(Vec::new());
    }
    if !matches!(request.action.as_str(), "home" | "move_axes") {
        return Err(invalid_printer_control());
    }
    Ok(required.clone())
}
