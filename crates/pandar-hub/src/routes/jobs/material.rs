use serde::Serialize;

use crate::{
    material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo, validate_mapping_len},
    routes::ApiError,
};

pub fn ams_mapping_json(value: Option<AmsMapping>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

pub fn ams_mapping2_json(value: Option<AmsMapping2>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

pub fn ams_mapping_info_json(value: Option<AmsMappingInfo>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

fn typed_mapping_json<T>(value: Option<Vec<T>>) -> Result<Option<String>, ApiError>
where
    T: Serialize,
{
    let Some(value) = value else {
        return Ok(None);
    };
    if !validate_mapping_len(value.len()) {
        return Err(ApiError::bad_request("invalid_material_mapping"));
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|_| ApiError::bad_request("invalid_material_mapping"))
}
