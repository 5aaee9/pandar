use std::collections::BTreeMap;

use pandar_core::{
    PrinterCoolingFan, PrinterCoolingFanKind, PrinterCoolingMode, PrinterCoolingSystem,
};

use crate::machine::mqtt::report::snapshot::{ScalarValue, SnapshotPrint};

pub(super) fn cooling_system_from_report(
    print: Option<&SnapshotPrint>,
) -> Option<PrinterCoolingSystem> {
    let print = print?;
    let mut fans = BTreeMap::new();
    let mode = print
        .device
        .airduct
        .as_ref()
        .and_then(|airduct| match airduct.mode {
            Some(0) => Some(PrinterCoolingMode::Cooling),
            Some(1) => Some(PrinterCoolingMode::Heating),
            Some(2) => Some(PrinterCoolingMode::Exhaust),
            Some(3) => Some(PrinterCoolingMode::FullCooling),
            _ => None,
        });

    if let Some(airduct) = &print.device.airduct {
        for part in &airduct.parts {
            if part.id & 0xf != 0 {
                continue;
            }
            let Some(kind) = cooling_fan_kind((part.id >> 4) & 0xff) else {
                continue;
            };
            let speed_percent = (((part.state & 0xff) / 10).min(10) * 10) as u8;
            fans.insert(kind, speed_percent);
        }
    }

    if let Some(gear) = print.fan_gear.as_ref().and_then(scalar_u32) {
        let part = pwm_percent(gear & 0xff);
        let auxiliary = pwm_percent((gear >> 8) & 0xff);
        let chamber = pwm_percent((gear >> 16) & 0xff);
        fans.entry(PrinterCoolingFanKind::PartCooling)
            .or_insert(part);
        if print.support_aux_fan == Some(true) || auxiliary > 0 {
            fans.entry(PrinterCoolingFanKind::Auxiliary)
                .or_insert(auxiliary);
        }
        if print.support_chamber_fan == Some(true) || chamber > 0 {
            fans.entry(PrinterCoolingFanKind::Chamber)
                .or_insert(chamber);
        }
    } else {
        insert_legacy_fan(
            &mut fans,
            PrinterCoolingFanKind::PartCooling,
            print.cooling_fan_speed.as_ref(),
        );
        insert_legacy_fan(
            &mut fans,
            PrinterCoolingFanKind::Auxiliary,
            print.big_fan1_speed.as_ref(),
        );
        insert_legacy_fan(
            &mut fans,
            PrinterCoolingFanKind::Chamber,
            print.big_fan2_speed.as_ref(),
        );
    }

    if mode.is_none() && fans.is_empty() {
        return None;
    }

    Some(PrinterCoolingSystem {
        mode,
        fans: fans
            .into_iter()
            .map(|(kind, speed_percent)| PrinterCoolingFan {
                kind,
                speed_percent,
            })
            .collect(),
    })
}

fn cooling_fan_kind(id: u32) -> Option<PrinterCoolingFanKind> {
    match id {
        0 => Some(PrinterCoolingFanKind::Hotend),
        1 => Some(PrinterCoolingFanKind::PartCooling),
        2 => Some(PrinterCoolingFanKind::Auxiliary),
        3 => Some(PrinterCoolingFanKind::Chamber),
        4 => Some(PrinterCoolingFanKind::HotendSecond),
        5 => Some(PrinterCoolingFanKind::Controller),
        6 => Some(PrinterCoolingFanKind::InnerLoop),
        10 => Some(PrinterCoolingFanKind::AuxiliarySecond),
        _ => None,
    }
}

fn insert_legacy_fan(
    fans: &mut BTreeMap<PrinterCoolingFanKind, u8>,
    kind: PrinterCoolingFanKind,
    value: Option<&ScalarValue>,
) {
    let Some(level) = value.and_then(scalar_u32).filter(|level| *level <= 15) else {
        return;
    };
    fans.entry(kind).or_insert(((level * 2 / 3) * 10) as u8);
}

fn pwm_percent(value: u32) -> u8 {
    (((value * 10 + 127) / 255).min(10) * 10) as u8
}

fn scalar_u32(value: &ScalarValue) -> Option<u32> {
    match value {
        ScalarValue::Number(value) => value.as_u64().and_then(|value| value.try_into().ok()),
        ScalarValue::String(value) => value.trim().parse().ok(),
    }
}
