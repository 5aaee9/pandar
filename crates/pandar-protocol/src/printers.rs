use pandar_core::{BambuDeviceFeatures, BambuNozzleSystem, PrinterCoolingSystem};

use crate::agent::v1::{
    PrinterCoolingFan as ProtoCoolingFan, PrinterCoolingFanKind as ProtoFanKind,
    PrinterCoolingMode as ProtoMode, PrinterCoolingSystem as ProtoCoolingSystem,
    PrinterDeviceFeatures as ProtoDeviceFeatures, PrinterNozzleDevice as ProtoNozzleDevice,
    PrinterNozzleHolder as ProtoNozzleHolder, PrinterNozzleInfo as ProtoNozzleInfo,
    PrinterNozzleSystem as ProtoNozzleSystem,
};

/// Convert the primary and secondary device-feature sets into their wire form
/// (agent → hub). `None` when neither side reported features.
pub fn proto_device_features(
    fun: Option<BambuDeviceFeatures>,
    fun2: Option<BambuDeviceFeatures>,
) -> Option<ProtoDeviceFeatures> {
    (fun.is_some() || fun2.is_some()).then(|| ProtoDeviceFeatures {
        bambu_fun_bits: fun.map(BambuDeviceFeatures::bits),
        bambu_fun2_bits: fun2.map(BambuDeviceFeatures::bits),
    })
}

/// Convert a wire device-feature snapshot into its primary and secondary
/// domain sets (hub ← agent).
pub fn core_device_features(
    features: ProtoDeviceFeatures,
) -> (Option<BambuDeviceFeatures>, Option<BambuDeviceFeatures>) {
    (
        features.bambu_fun_bits.map(BambuDeviceFeatures::from_bits),
        features.bambu_fun2_bits.map(BambuDeviceFeatures::from_bits),
    )
}

/// Convert a cooling system into its wire form (agent → hub).
pub fn proto_cooling_system(system: PrinterCoolingSystem) -> ProtoCoolingSystem {
    use pandar_core::{PrinterCoolingFanKind as FanKind, PrinterCoolingMode as Mode};

    ProtoCoolingSystem {
        mode: system.mode.map(|mode| match mode {
            Mode::Cooling => ProtoMode::Cooling as i32,
            Mode::Heating => ProtoMode::Heating as i32,
            Mode::Exhaust => ProtoMode::Exhaust as i32,
            Mode::FullCooling => ProtoMode::FullCooling as i32,
        }),
        fans: system
            .fans
            .into_iter()
            .map(|fan| ProtoCoolingFan {
                kind: match fan.kind {
                    FanKind::Hotend => ProtoFanKind::Hotend,
                    FanKind::PartCooling => ProtoFanKind::PartCooling,
                    FanKind::Auxiliary => ProtoFanKind::Auxiliary,
                    FanKind::Chamber => ProtoFanKind::Chamber,
                    FanKind::HotendSecond => ProtoFanKind::HotendSecond,
                    FanKind::Controller => ProtoFanKind::Controller,
                    FanKind::InnerLoop => ProtoFanKind::InnerLoop,
                    FanKind::AuxiliarySecond => ProtoFanKind::AuxiliarySecond,
                } as i32,
                speed_percent: fan.speed_percent.into(),
            })
            .collect(),
    }
}

/// Convert a nozzle system into its wire form (agent → hub).
pub fn proto_nozzle_system(system: BambuNozzleSystem) -> ProtoNozzleSystem {
    ProtoNozzleSystem {
        nozzle: Some(ProtoNozzleDevice {
            exist: system.nozzle.exist,
            state: system.nozzle.state,
            src_id: system.nozzle.src_id,
            tar_id: system.nozzle.tar_id,
            info: system
                .nozzle
                .info
                .into_iter()
                .map(|nozzle| ProtoNozzleInfo {
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
        holder: system.holder.map(|holder| ProtoNozzleHolder {
            stat: holder.stat,
            pos: holder.pos,
            info: holder.info,
        }),
    }
}
