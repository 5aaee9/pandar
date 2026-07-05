use serde_json::Value;

use crate::machine::{BambuPrinterEndpoint, MachineNozzleTemperature, MachineSnapshot};

pub fn snapshot_from_report(endpoint: &BambuPrinterEndpoint, report: &Value) -> MachineSnapshot {
    let print = report.get("print").unwrap_or(&Value::Null);
    let state = ["/print/gcode_state", "/print/state", "/state"]
        .into_iter()
        .find_map(|path| report.pointer(path).and_then(Value::as_str))
        .unwrap_or("unknown");
    let (packed_bed_temperature, packed_bed_target_temperature) =
        packed_temperature_pair(print.pointer("/device/bed_temp"));
    let (packed_chamber_temperature, _) =
        packed_temperature_pair(print.pointer("/device/ctc/info/temp"));

    MachineSnapshot {
        serial: endpoint.serial.clone(),
        host: Some(endpoint.host.clone()),
        access_code: Some(endpoint.access_code.clone()),
        name: endpoint
            .name
            .clone()
            .unwrap_or_else(|| endpoint.serial.clone()),
        model: endpoint.model.clone(),
        state: state.to_string(),
        nozzle_temperatures: nozzle_temperatures_from_report(print),
        active_nozzle: active_nozzle_from_report(print),
        bed_temperature_celsius: temperature_string(
            print
                .get("bed_temper")
                .or_else(|| print.get("bed_temp"))
                .or_else(|| print.get("bed_temperature")),
        )
        .or(packed_bed_temperature),
        bed_target_temperature_celsius: temperature_string(
            print
                .get("bed_target_temper")
                .or_else(|| print.get("target_bed_temper"))
                .or_else(|| print.get("bed_target_temperature")),
        )
        .or(packed_bed_target_temperature),
        chamber_temperature_celsius: temperature_string(
            print
                .get("chamber_temper")
                .or_else(|| print.get("chamber_temp"))
                .or_else(|| print.get("chamber_temperature")),
        )
        .or(packed_chamber_temperature),
        chamber_light_on: chamber_light_on_from_report(print),
    }
}

fn chamber_light_on_from_report(print: &Value) -> Option<bool> {
    let lights = print.get("lights_report")?.as_array()?;
    lights
        .iter()
        .find(|light| light.get("node").and_then(Value::as_str) == Some("chamber_light"))
        .and_then(|light| light.get("mode").and_then(Value::as_str))
        .map(|mode| mode == "on")
}

fn active_nozzle_from_report(print: &Value) -> Option<String> {
    let state = print.pointer("/device/extruder/state")?.as_u64()?;
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

fn nozzle_temperatures_from_report(print: &Value) -> Vec<MachineNozzleTemperature> {
    if let Some(nozzles) = nozzle_temperatures_from_v2_report(print) {
        return nozzles;
    }

    let left = MachineNozzleTemperature {
        label: None,
        current_celsius: temperature_string(
            print
                .get("nozzle_temper")
                .or_else(|| print.get("nozzle_temp"))
                .or_else(|| print.get("nozzle_temperature")),
        ),
        target_celsius: temperature_string(
            print
                .get("nozzle_target_temper")
                .or_else(|| print.get("target_nozzle_temper"))
                .or_else(|| print.get("nozzle_target_temperature")),
        ),
    };
    let right = MachineNozzleTemperature {
        label: Some("R".to_owned()),
        current_celsius: temperature_string(
            print
                .get("nozzle_temper2")
                .or_else(|| print.get("right_nozzle_temper"))
                .or_else(|| print.get("nozzle_temp2")),
        ),
        target_celsius: temperature_string(
            print
                .get("nozzle_target_temper2")
                .or_else(|| print.get("right_nozzle_target_temper"))
                .or_else(|| print.get("target_nozzle_temper2")),
        ),
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

fn nozzle_temperatures_from_v2_report(print: &Value) -> Option<Vec<MachineNozzleTemperature>> {
    let extruder = print.pointer("/device/extruder")?;
    let info = extruder.get("info")?.as_array()?;
    let total = extruder
        .get("state")
        .and_then(Value::as_u64)
        .map(|value| value & 0xf)
        .unwrap_or(info.len() as u64);
    let mut nozzles = Vec::new();

    for (index, item) in info.iter().enumerate() {
        let (current_celsius, target_celsius) = packed_temperature_pair(item.get("temp"));
        if current_celsius.is_none() && target_celsius.is_none() {
            continue;
        }
        let id = item
            .get("id")
            .and_then(Value::as_u64)
            .unwrap_or(index as u64);
        nozzles.push((
            nozzle_sort_key(total, id),
            MachineNozzleTemperature {
                label: nozzle_label(total, id),
                current_celsius,
                target_celsius,
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

fn temperature_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Number(number) => number.as_f64().and_then(temperature_string_from_number),
        Value::String(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty() && trimmed != "-1").then(|| trimmed.to_owned())
        }
        _ => None,
    }
}

fn packed_temperature_pair(value: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(bits) = value.and_then(Value::as_u64) else {
        return (None, None);
    };
    (
        temperature_string_from_number((bits & 0xffff) as f64),
        temperature_string_from_number(((bits >> 16) & 0xffff) as f64),
    )
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
