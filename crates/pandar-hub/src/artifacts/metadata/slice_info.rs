use std::collections::BTreeSet;

use anyhow::Context;
use quick_xml::{
    Reader, XmlVersion,
    events::{BytesStart, Event, attributes::Attribute},
};

use super::{Draft, FilamentMetadata, MAX_FILAMENTS_PER_PLATE, MAX_OBJECTS_PER_PLATE, PlateSource};

pub(super) fn parse(contents: &str, draft: &mut Draft) -> anyhow::Result<()> {
    let mut reader = Reader::from_str(contents);
    let mut current_plate = None;
    let mut inside_plate = false;
    let mut slice_objects = BTreeSet::new();
    let mut filament_nozzle_ids = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if event.name().as_ref() == b"plate" => {
                current_plate = None;
                filament_nozzle_ids.clear();
                inside_plate = true;
            }
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) if inside_plate => {
                parse_plate_element(
                    &reader,
                    &event,
                    &mut current_plate,
                    &mut slice_objects,
                    &mut filament_nozzle_ids,
                    draft,
                )?;
            }
            Ok(Event::End(event)) if event.name().as_ref() == b"plate" => {
                current_plate = None;
                inside_plate = false;
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(err).context("failed to parse slice_info.config"),
            _ => {}
        }
    }

    Ok(())
}

fn parse_plate_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    current_plate: &mut Option<u32>,
    slice_objects: &mut BTreeSet<u32>,
    filament_nozzle_ids: &mut Vec<Option<u8>>,
    draft: &mut Draft,
) -> anyhow::Result<()> {
    match event.name().as_ref() {
        b"metadata" => {
            let (key, value) = key_value(reader, event)?;
            let Some(value) = value else {
                return Ok(());
            };
            match key.as_deref() {
                Some("index") => {
                    *current_plate = parse_u32(&value);
                    if let Some(plate_id) = *current_plate {
                        draft.ensure_plate(plate_id, PlateSource::SliceInfo);
                    }
                }
                Some("prediction") => {
                    if let Some(plate_id) = *current_plate
                        && let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo)
                        && plate.metadata.estimated_time_seconds.is_none()
                    {
                        plate.metadata.estimated_time_seconds = parse_u32(&value);
                    }
                }
                Some("weight") => {
                    if let Some(plate_id) = *current_plate
                        && let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo)
                        && plate.metadata.filament_weight_grams.is_none()
                    {
                        plate.metadata.filament_weight_grams = parse_f64(&value);
                    }
                }
                Some("filament_maps") => {
                    *filament_nozzle_ids = parse_filament_maps(&value);
                }
                _ => {}
            }
        }
        b"object" => {
            let Some(plate_id) = *current_plate else {
                return Ok(());
            };
            let Some(name) =
                attribute(reader, event, b"name")?.filter(|value| !value.trim().is_empty())
            else {
                return Ok(());
            };
            let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo) else {
                return Ok(());
            };
            if slice_objects.insert(plate_id) {
                plate.metadata.object_count = 0;
                plate.metadata.objects.clear();
            }
            push_object(&mut plate.metadata, name);
        }
        b"filament" => {
            let Some(plate_id) = *current_plate else {
                return Ok(());
            };
            let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo) else {
                return Ok(());
            };
            if plate.metadata.filaments.len() >= MAX_FILAMENTS_PER_PLATE {
                draft.warnings.insert("filament_limit_reached");
                return Ok(());
            }

            let mut filament = FilamentMetadata {
                filament_id: None,
                tray_info_idx: None,
                nozzle_id: None,
                filament_type: None,
                color: None,
                used_grams: None,
                used_meters: None,
            };
            for attr in event.attributes().flatten() {
                let value = attr_value(reader, &attr)?;
                match attr.key.as_ref() {
                    b"id" => filament.filament_id = Some(value),
                    b"tray_info_idx" => filament.tray_info_idx = Some(value),
                    b"type" => filament.filament_type = Some(value),
                    b"color" => filament.color = Some(value),
                    b"used_g" => filament.used_grams = parse_f64(&value),
                    b"used_m" => filament.used_meters = parse_f64(&value),
                    _ => {}
                }
            }
            filament.nozzle_id = filament
                .filament_id
                .as_deref()
                .and_then(|id| id.parse::<usize>().ok())
                .and_then(|id| id.checked_sub(1))
                .and_then(|index| filament_nozzle_ids.get(index))
                .copied()
                .flatten();
            plate.metadata.filaments.push(filament);
        }
        _ => {}
    }

    Ok(())
}

fn key_value(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
) -> quick_xml::Result<(Option<String>, Option<String>)> {
    let mut key = None;
    let mut value = None;
    for attr in event.attributes().flatten() {
        let decoded = attr_value(reader, &attr)?;
        match attr.key.as_ref() {
            b"key" => key = Some(decoded),
            b"value" => value = Some(decoded),
            _ => {}
        }
    }
    Ok((key, value))
}

fn attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    key: &[u8],
) -> quick_xml::Result<Option<String>> {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == key {
            return attr_value(reader, &attr).map(Some);
        }
    }
    Ok(None)
}

fn attr_value(reader: &Reader<&[u8]>, attr: &Attribute<'_>) -> quick_xml::Result<String> {
    attr.decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
        .map(|value| value.to_string())
}

fn push_object(plate: &mut super::PlateMetadata, name: String) {
    plate.object_count += 1;
    if plate.objects.len() < MAX_OBJECTS_PER_PLATE {
        plate.objects.push(name);
    }
}

fn parse_u32(value: &str) -> Option<u32> {
    value.parse::<u32>().ok().filter(|value| *value > 0)
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn parse_filament_maps(value: &str) -> Vec<Option<u8>> {
    value
        .split_whitespace()
        .map(|value| match value {
            "1" => Some(1),
            "2" => Some(0),
            _ => None,
        })
        .collect()
}
