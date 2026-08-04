use pandar_core::{
    H2cAutoMappingFilamentInfo, H2cAutoMappingGroupInfo, H2cAutoMappingNozzleInfo,
    H2cAutoNozzleMappingRequest,
};

use crate::{
    machine::PrinterOperation as MachinePrinterOperation,
    protocol::agent::v1::GetAutoNozzleMappingOperation,
};

const MAX_HOLDER_CTRL_ACTION: u32 = 2;
const RACK_NOZZLE_ID_ALL: u32 = 0xff;

pub(super) fn parse_nozzle_holder_ctrl(action: u32) -> anyhow::Result<MachinePrinterOperation> {
    if action > MAX_HOLDER_CTRL_ACTION {
        anyhow::bail!("invalid H2C nozzle_holder_ctrl action; expected 0..=2");
    }
    Ok(MachinePrinterOperation::NozzleHolderCtrl { action })
}

pub(super) fn parse_nozzle_info_confirm(id: u32) -> anyhow::Result<MachinePrinterOperation> {
    if !valid_rack_nozzle_id(id) {
        anyhow::bail!("invalid H2C nozzle_info_confirm id; expected 16..=21 or 255");
    }
    Ok(MachinePrinterOperation::NozzleInfoConfirm { id })
}

pub(super) fn parse_holder_nozzle_refresh(id: u32) -> anyhow::Result<MachinePrinterOperation> {
    if !valid_rack_nozzle_id(id) {
        anyhow::bail!("invalid H2C holder_nozzle_refresh id; expected 16..=21 or 255");
    }
    Ok(MachinePrinterOperation::HolderNozzleRefresh { id })
}

fn valid_rack_nozzle_id(id: u32) -> bool {
    (16..=21).contains(&id) || id == RACK_NOZZLE_ID_ALL
}

pub(super) fn parse_auto_nozzle_mapping(
    operation: &GetAutoNozzleMappingOperation,
) -> anyhow::Result<MachinePrinterOperation> {
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
    if !request.is_valid() {
        anyhow::bail!("invalid H2C auto nozzle mapping request");
    }
    Ok(MachinePrinterOperation::GetAutoNozzleMapping(request))
}
