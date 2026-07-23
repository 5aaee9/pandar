mod schema;

use serde_json::Value;

use crate::machine::{
    BambuPrinterEndpoint, MachineNozzleTemperature, MachineSnapshot, types::decode_json_payload,
};

use self::schema::{NozzleInfo, ScalarValue, SnapshotPrint, TemperatureValue};
use super::device_features::device_feature_observation;

pub(crate) use self::schema::SnapshotReport;

pub fn snapshot_from_report(endpoint: &BambuPrinterEndpoint, report: &Value) -> MachineSnapshot {
    let report = parse_snapshot_report(report);
    snapshot_from_parsed_report(endpoint, report.as_ref())
}

pub(crate) fn parse_snapshot_report(report: &Value) -> Option<SnapshotReport> {
    decode_json_payload(report)
}

pub(crate) fn snapshot_from_parsed_report(
    endpoint: &BambuPrinterEndpoint,
    report: Option<&SnapshotReport>,
) -> MachineSnapshot {
    let print = report.and_then(|report| report.print.as_ref());
    let state = print
        .and_then(|print| {
            trimmed_string(print.gcode_state.as_ref())
                .or_else(|| trimmed_string(print.state.as_ref()))
        })
        .or_else(|| report.and_then(|report| trimmed_string(report.state.as_ref())));
    let (packed_bed_temperature, packed_bed_target_temperature) =
        packed_temperature_pair(print.and_then(|print| print.device.bed_temp.as_ref()));
    let (packed_chamber_temperature, packed_chamber_target_temperature) =
        packed_temperature_pair(print.and_then(|print| print.device.ctc.info.temp.as_ref()));

    MachineSnapshot {
        serial: endpoint.serial.clone(),
        host: Some(endpoint.host.clone()),
        access_code: Some(endpoint.access_code.clone()),
        name: endpoint
            .name
            .clone()
            .unwrap_or_else(|| endpoint.serial.clone()),
        model: endpoint.model.clone(),
        state,
        nozzle_temperatures: nozzle_temperatures_from_report(print),
        active_nozzle: active_nozzle_from_report(print),
        bed_temperature_celsius: temperature_string(
            print.and_then(|print| print.bed_temper.as_ref()),
        )
        .or(packed_bed_temperature),
        bed_target_temperature_celsius: temperature_string(
            print.and_then(|print| print.bed_target_temper.as_ref()),
        )
        .or(packed_bed_target_temperature),
        chamber_temperature_celsius: temperature_string(
            print.and_then(|print| print.chamber_temper.as_ref()),
        )
        .or(packed_chamber_temperature),
        chamber_target_temperature_celsius: packed_chamber_target_temperature.or_else(|| {
            temperature_string(print.and_then(|print| print.chamber_target_temper.as_ref()))
        }),
        chamber_light_on: chamber_light_on_from_report(print),
        device_features: report
            .and_then(|report| device_feature_observation(&endpoint.serial, report).ok())
            .flatten(),
        telemetry_authoritative: false,
    }
}

fn chamber_light_on_from_report(print: Option<&SnapshotPrint>) -> Option<bool> {
    print?
        .lights_report
        .iter()
        .find(|light| light.node.as_deref() == Some("chamber_light"))
        .and_then(|light| light.mode.as_deref())
        .and_then(|mode| match mode {
            "on" | "flashing" => Some(true),
            "off" => Some(false),
            _ => None,
        })
}

fn active_nozzle_from_report(print: Option<&SnapshotPrint>) -> Option<String> {
    let state = print?.device.extruder.state?;
    let total = state & 0xf;
    if total <= 1 {
        return None;
    }

    Some(if ((state >> 4) & 0xf) == 1 {
        "L".to_owned()
    } else {
        "R".to_owned()
    })
}

fn nozzle_temperatures_from_report(print: Option<&SnapshotPrint>) -> Vec<MachineNozzleTemperature> {
    let Some(print) = print else {
        return Vec::new();
    };

    if let Some(nozzles) = nozzle_temperatures_from_v2_report(print) {
        return nozzles;
    }

    let left = MachineNozzleTemperature {
        label: None,
        current_celsius: temperature_string(print.nozzle_temper.as_ref()),
        target_celsius: temperature_string(print.nozzle_target_temper.as_ref()),
        diameter_mm: None,
        nozzle_type: None,
    };
    let right = MachineNozzleTemperature {
        label: Some("R".to_owned()),
        current_celsius: temperature_string(print.nozzle_temper2.as_ref()),
        target_celsius: temperature_string(print.nozzle_target_temper2.as_ref()),
        diameter_mm: None,
        nozzle_type: None,
    };

    if right.current_celsius.is_some() || right.target_celsius.is_some() {
        vec![
            MachineNozzleTemperature {
                label: Some("L".to_owned()),
                ..left
            },
            right,
        ]
    } else if left.current_celsius.is_some() || left.target_celsius.is_some() {
        vec![left]
    } else {
        Vec::new()
    }
}

fn nozzle_temperatures_from_v2_report(
    print: &SnapshotPrint,
) -> Option<Vec<MachineNozzleTemperature>> {
    let info = &print.device.extruder.info;
    if info.is_empty() {
        return None;
    }
    let total = print
        .device
        .extruder
        .state
        .map(|value| value & 0xf)
        .unwrap_or(info.len() as u64);
    let mut nozzles = Vec::new();

    for (index, item) in info.iter().enumerate() {
        let (current_celsius, target_celsius) = packed_temperature_pair(item.temp.as_ref());
        if current_celsius.is_none() && target_celsius.is_none() {
            continue;
        }
        let id = item.id.unwrap_or(index as u64);
        nozzles.push((
            nozzle_sort_key(total, id),
            MachineNozzleTemperature {
                label: nozzle_label(total, id),
                current_celsius,
                target_celsius,
                diameter_mm: nozzle_diameter_for_id(print, id),
                nozzle_type: nozzle_type_for_id(print, id),
            },
        ));
    }

    nozzles.sort_by_key(|(key, _)| *key);
    Some(
        nozzles
            .into_iter()
            .map(|(_, temperature)| temperature)
            .collect(),
    )
}

fn nozzle_diameter_for_id(print: &SnapshotPrint, id: u64) -> Option<String> {
    match nozzle_info_for_id(print, id)?.diameter.as_ref()? {
        ScalarValue::Number(number) => number.as_f64().map(|value| {
            let text = value.to_string();
            text.strip_suffix(".0").unwrap_or(&text).to_owned()
        }),
        ScalarValue::String(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
    }
}

fn nozzle_type_for_id(print: &SnapshotPrint, id: u64) -> Option<String> {
    let nozzle = nozzle_info_for_id(print, id)?;
    let raw = nozzle
        .kind
        .as_deref()
        .or(nozzle.nozzle_type.as_deref())?
        .trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_owned())
}

fn nozzle_info_for_id(print: &SnapshotPrint, id: u64) -> Option<&NozzleInfo> {
    print
        .device
        .nozzle
        .info
        .iter()
        .find(|item| item.id == Some(id))
}

fn nozzle_label(total: u64, id: u64) -> Option<String> {
    (total > 1).then(|| match id {
        1 => "L".to_owned(),
        0 => "R".to_owned(),
        value => (value + 1).to_string(),
    })
}

fn nozzle_sort_key(total: u64, id: u64) -> u64 {
    if total <= 1 {
        return id;
    }
    match id {
        1 => 0,
        0 => 1,
        value => value + 1,
    }
}

fn temperature_string(value: Option<&TemperatureValue>) -> Option<String> {
    match value? {
        TemperatureValue::Number(number) => {
            number.as_f64().and_then(temperature_string_from_number)
        }
        TemperatureValue::String(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty() && trimmed != "-1").then(|| trimmed.to_owned())
        }
    }
}

fn packed_temperature_pair(value: Option<&TemperatureValue>) -> (Option<String>, Option<String>) {
    let Some(TemperatureValue::Number(number)) = value else {
        return (None, None);
    };
    let Some(bits) = number.as_u64() else {
        return (None, None);
    };
    (
        temperature_string_from_number((bits & 0xffff) as f64),
        temperature_string_from_number(((bits >> 16) & 0xffff) as f64),
    )
}

fn trimmed_string(value: Option<&ScalarValue>) -> Option<String> {
    let ScalarValue::String(value) = value? else {
        return None;
    };
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn temperature_string_from_number(value: f64) -> Option<String> {
    (value.is_finite() && value >= 0.0).then(|| {
        if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.1}")
        }
    })
}
