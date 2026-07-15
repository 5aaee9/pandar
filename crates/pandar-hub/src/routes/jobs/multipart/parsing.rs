use pandar_core::PrintCalibrationMode;
use serde::de::DeserializeOwned;

use crate::routes::ApiError;

pub(super) fn parse_i64(value: &str) -> Result<i64, ApiError> {
    value
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("bad_request"))
}

pub(super) fn parse_bool(value: &str) -> Result<bool, ApiError> {
    value
        .parse::<bool>()
        .map_err(|_| ApiError::bad_request("bad_request"))
}

pub(super) fn parse_calibration_mode(value: &str) -> Result<PrintCalibrationMode, ApiError> {
    value
        .parse::<u8>()
        .map_err(|_| ApiError::bad_request("bad_request"))
        .and_then(|value| {
            PrintCalibrationMode::try_from(value).map_err(|_| ApiError::bad_request("bad_request"))
        })
}

pub(super) fn parse_optional_json_field<T>(value: &str) -> Result<Option<T>, ApiError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(value).map_err(|_| ApiError::bad_request("invalid_material_mapping"))
}

pub(super) fn required<T>(value: Option<T>) -> Result<T, ApiError> {
    value.ok_or_else(|| ApiError::bad_request("bad_request"))
}
