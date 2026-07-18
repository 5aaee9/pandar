use pandar_core::PrintCalibrationMode;
use serde::Deserialize;

use crate::{
    material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo},
    repositories::DuplicatePrintJob,
    routes::ApiError,
};

use super::{material, parse_printer_id, validated_plate_id};

#[derive(Debug, Deserialize)]
pub(crate) struct ReprintJobRequest {
    pub(super) reason: Option<String>,
    #[serde(flatten)]
    pub(super) overrides: DuplicateJobRequest,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DuplicateJobRequest {
    printer_id: Option<String>,
    plate_id: Option<i64>,
    use_ams: Option<bool>,
    bed_leveling: Option<bool>,
    auto_bed_leveling: Option<PrintCalibrationMode>,
    flow_cali: Option<bool>,
    auto_flow_cali: Option<PrintCalibrationMode>,
    auto_offset_cali: Option<PrintCalibrationMode>,
    timelapse: Option<bool>,
    ams_mapping: Option<AmsMapping>,
    ams_mapping2: Option<AmsMapping2>,
    ams_mapping_info: Option<AmsMappingInfo>,
}

impl DuplicateJobRequest {
    pub(super) fn into_repository(self) -> Result<DuplicatePrintJob, ApiError> {
        if let Some(printer_id) = &self.printer_id {
            parse_printer_id(printer_id)?;
        }
        let replace_ams_mappings = self.ams_mapping.is_some()
            && self.ams_mapping2.is_some()
            && self.ams_mapping_info.is_some();
        Ok(DuplicatePrintJob {
            printer_id: self.printer_id,
            plate_id: self.plate_id.map(validated_plate_id).transpose()?,
            use_ams: self.use_ams,
            bed_leveling: self.bed_leveling,
            auto_bed_leveling: self.auto_bed_leveling,
            flow_cali: self.flow_cali,
            auto_flow_cali: self.auto_flow_cali,
            auto_offset_cali: self.auto_offset_cali,
            timelapse: self.timelapse,
            replace_ams_mappings,
            ams_mapping_json: material::ams_mapping_json(non_empty(self.ams_mapping))?,
            ams_mapping2_json: material::ams_mapping2_json(non_empty(self.ams_mapping2))?,
            ams_mapping_info_json: material::ams_mapping_info_json(non_empty(
                self.ams_mapping_info,
            ))?,
        })
    }
}

fn non_empty<T>(value: Option<Vec<T>>) -> Option<Vec<T>> {
    value.filter(|items| !items.is_empty())
}
