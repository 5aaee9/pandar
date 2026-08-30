use pandar_core::{
    H2cAutoMappingFilamentInfo, H2cAutoMappingGroupInfo, H2cAutoMappingNozzleInfo,
    H2cAutoNozzleMappingRequest, PrinterOperation,
};
use pandar_protocol::agent::v1::GetAutoNozzleMappingOperation;

pub(super) fn parse_auto_nozzle_mapping(
    operation: &GetAutoNozzleMappingOperation,
) -> anyhow::Result<PrinterOperation> {
    let version = operation.version.and_then(|value| u8::try_from(value).ok());
    let request = H2cAutoNozzleMappingRequest {
        command: "get_auto_nozzle_mapping".to_owned(),
        sequence_id: operation.sequence_id.clone(),
        version,
        calibration: operation.calibration,
        extrude_cali_manual_mode: operation.extrude_cali_manual_mode,
        filament_seq: (version != Some(1)).then(|| operation.filament_seq.clone()),
        ams_mapping: (version != Some(1)).then(|| operation.ams_mapping.clone()),
        fila_info: (version != Some(1)).then(|| {
            operation
                .fila_info
                .iter()
                .map(|value| H2cAutoMappingFilamentInfo {
                    id: value.id,
                    direction: u8::try_from(value.direction).unwrap_or(u8::MAX),
                    group: value.group,
                    nozzle_d: value.nozzle_d.clone(),
                    nozzle_v: value.nozzle_v.clone(),
                    cate: value.cate.clone(),
                    color: value.color.clone(),
                })
                .collect()
        }),
        nozzle_info: (version != Some(1)).then(|| {
            operation
                .nozzle_info
                .iter()
                .map(|value| H2cAutoMappingNozzleInfo {
                    pos: value.pos,
                    nozzle_d: value.nozzle_d.clone(),
                    nozzle_v: value.nozzle_v.clone(),
                    wear: value.wear,
                    cate: value.cate.clone(),
                    color: value.color.clone(),
                })
                .collect()
        }),
        group_info: (version == Some(1)).then(|| {
            operation
                .group_info
                .iter()
                .map(|value| H2cAutoMappingGroupInfo {
                    id: value.id,
                    ext: u8::try_from(value.ext).unwrap_or(u8::MAX),
                    dia: value.dia,
                    vol: value.vol.clone(),
                })
                .collect()
        }),
    };
    Ok(PrinterOperation::GetAutoNozzleMapping { request })
}
