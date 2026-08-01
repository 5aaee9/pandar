use pandar_core::{StudioPrintMetadata, StudioPrintMetadataV1};

use crate::routes::ApiError;

use super::{
    MultipartPrintKind,
    parsing::{parse_bool, parse_i64, parse_optional_json_field},
    types::MultipartPrintFields,
};

pub(super) fn parse_field(
    fields: &mut MultipartPrintFields,
    name: &str,
    text: &str,
) -> Result<(), ApiError> {
    match name {
        "task_name" => fields.task_name = Some(text.to_owned()),
        "project_name" => fields.project_name = Some(text.to_owned()),
        "preset_name" => fields.preset_name = Some(text.to_owned()),
        "config_plate_index" => {
            fields.config_plate_index = Some(parse_u32(text)?);
        }
        "nozzle_mapping" => {
            fields.nozzle_mapping = parse_optional_json_field(text)?;
        }
        "nozzles_info" => {
            fields.nozzles_info = parse_optional_json_field(text)?;
        }
        "connection_type" => fields.connection_type = Some(text.to_owned()),
        "comments" => fields.comments = Some(text.to_owned()),
        "origin_profile_id" => fields.origin_profile_id = Some(parse_i64(text)?),
        "stl_design_id" => fields.stl_design_id = Some(parse_i64(text)?),
        "origin_model_id" => fields.origin_model_id = Some(text.to_owned()),
        "print_type" => fields.print_type = Some(text.to_owned()),
        "dev_name" => fields.submitted_device_name = Some(text.to_owned()),
        "vibration_cali" => fields.vibration_cali = Some(parse_bool(text)?),
        "layer_inspect" => fields.layer_inspect = Some(parse_bool(text)?),
        "timelapse_use_internal" => {
            fields.timelapse_use_internal = Some(parse_bool(text)?);
        }
        "bed_type" => fields.bed_type = Some(text.to_owned()),
        "extruder_cali_manual_mode" => {
            fields.extruder_cali_manual_mode = Some(parse_manual_mode(text)?);
        }
        "try_emmc_print" => fields.try_emmc_print = Some(parse_bool(text)?),
        "svc_context" => fields.svc_context = Some(text.to_owned()),
        "slicer_uid" => fields.slicer_uid = Some(text.to_owned()),
        _ => fields.unknown_field = true,
    }
    Ok(())
}

pub(super) fn validate_h2c_admission(
    model: Option<&str>,
    kind: MultipartPrintKind,
    metadata: Option<&StudioPrintMetadata>,
) -> Result<(), ApiError> {
    if model
        .and_then(pandar_core::compatibility::normalize_model)
        .as_deref()
        == Some("H2C")
        && (matches!(kind, MultipartPrintKind::Web)
            || !metadata.is_some_and(|metadata| {
                pandar_core::valid_h2c_nozzle_mapping(metadata.nozzle_mapping())
            }))
    {
        return Err(ApiError::bad_request("h2c_nozzle_mapping_required"));
    }
    Ok(())
}

pub(super) fn metadata(fields: &MultipartPrintFields) -> Result<StudioPrintMetadata, ApiError> {
    if fields.unknown_field {
        return Err(invalid_metadata());
    }
    let connection_type = required_metadata(fields.connection_type.clone())?;
    let print_type = required_metadata(fields.print_type.clone())?;
    if connection_type != "cloud" || print_type != "from_normal" {
        return Err(invalid_metadata());
    }
    Ok(StudioPrintMetadata::V1(StudioPrintMetadataV1 {
        task_name: required_metadata(fields.task_name.clone())?,
        project_name: required_metadata(fields.project_name.clone())?,
        preset_name: required_metadata(fields.preset_name.clone())?,
        config_plate_index: fields.config_plate_index,
        nozzle_mapping: required_metadata(fields.nozzle_mapping.clone())?,
        ams_mapping: required_metadata(fields.ams_mapping.clone())?,
        ams_mapping2: required_metadata(fields.ams_mapping2.clone())?,
        ams_mapping_info: required_metadata(fields.ams_mapping_info.clone())?,
        nozzles_info: required_metadata(fields.nozzles_info.clone())?,
        connection_type,
        comments: required_metadata(fields.comments.clone())?,
        origin_profile_id: required_metadata(fields.origin_profile_id)?,
        stl_design_id: required_metadata(fields.stl_design_id)?,
        origin_model_id: required_metadata(fields.origin_model_id.clone())?,
        print_type,
        submitted_device_name: required_metadata(fields.submitted_device_name.clone())?,
        task_bed_leveling: required_metadata(fields.bed_leveling)?,
        task_flow_cali: required_metadata(fields.flow_cali)?,
        task_vibration_cali: required_metadata(fields.vibration_cali)?,
        task_layer_inspect: required_metadata(fields.layer_inspect)?,
        task_record_timelapse: required_metadata(fields.timelapse)?,
        task_timelapse_use_internal: required_metadata(fields.timelapse_use_internal)?,
        task_use_ams: required_metadata(fields.use_ams)?,
        task_bed_type: required_metadata(fields.bed_type.clone())?,
        auto_bed_leveling: required_metadata(fields.auto_bed_leveling)?,
        auto_flow_cali: required_metadata(fields.auto_flow_cali)?,
        auto_offset_cali: required_metadata(fields.auto_offset_cali)?,
        extruder_cali_manual_mode: required_metadata(fields.extruder_cali_manual_mode)?,
        try_emmc_print: required_metadata(fields.try_emmc_print)?,
        svc_context: required_metadata(fields.svc_context.clone())?,
        slicer_uid: required_metadata(fields.slicer_uid.clone())?,
    }))
}

fn parse_u32(value: &str) -> Result<u32, ApiError> {
    value.parse().map_err(|_| invalid_metadata())
}

fn parse_manual_mode(value: &str) -> Result<i8, ApiError> {
    let value = value.parse::<i8>().map_err(|_| invalid_metadata())?;
    (-1..=1)
        .contains(&value)
        .then_some(value)
        .ok_or_else(invalid_metadata)
}

fn required_metadata<T>(value: Option<T>) -> Result<T, ApiError> {
    value.ok_or_else(invalid_metadata)
}

fn invalid_metadata() -> ApiError {
    ApiError::bad_request("invalid_studio_print_metadata")
}
