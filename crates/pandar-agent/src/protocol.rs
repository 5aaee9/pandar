pub mod agent {
    pub mod v1 {
        tonic::include_proto!("pandar.agent.v1");
    }
}

pub(crate) fn proto_device_features(
    fun: Option<pandar_core::BambuDeviceFeatures>,
    fun2: Option<pandar_core::BambuDeviceFeatures>,
) -> Option<agent::v1::PrinterDeviceFeatures> {
    (fun.is_some() || fun2.is_some()).then(|| agent::v1::PrinterDeviceFeatures {
        bambu_fun_bits: fun.map(pandar_core::BambuDeviceFeatures::bits),
        bambu_fun2_bits: fun2.map(pandar_core::BambuDeviceFeatures::bits),
    })
}

pub(crate) fn proto_nozzle_system(
    system: pandar_core::BambuNozzleSystem,
) -> agent::v1::PrinterNozzleSystem {
    agent::v1::PrinterNozzleSystem {
        nozzle: Some(agent::v1::PrinterNozzleDevice {
            exist: system.nozzle.exist,
            state: system.nozzle.state,
            src_id: system.nozzle.src_id,
            tar_id: system.nozzle.tar_id,
            info: system
                .nozzle
                .info
                .into_iter()
                .map(|nozzle| agent::v1::PrinterNozzleInfo {
                    id: nozzle.id,
                    diameter: nozzle.diameter.get() as f32,
                    nozzle_type: nozzle.nozzle_type,
                    stat: nozzle.stat,
                    fila_id: nozzle.fila_id,
                    wear: nozzle.wear.map(|wear| wear.get() as f32),
                    print_time: nozzle.p_t,
                    color: nozzle.color_m,
                })
                .collect(),
        }),
        holder: system.holder.map(|holder| agent::v1::PrinterNozzleHolder {
            stat: holder.stat,
            pos: holder.pos,
            info: holder.info,
        }),
    }
}

#[cfg(test)]
mod tests;
