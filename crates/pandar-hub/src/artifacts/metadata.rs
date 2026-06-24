use std::{
    fs::File,
    io::{Read, Seek},
    path::Path,
};

use anyhow::Context;
use quick_xml::{Reader, XmlVersion, events::Event, events::attributes::Attribute};
use zip::ZipArchive;

mod types;

pub use types::{ArtifactMetadata, FilamentMetadata, PlateMetadata};
use types::{Draft, PlateSource};

const MAX_ZIP_ENTRIES: usize = 512;
const MAX_METADATA_FILE_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PLATES: usize = 64;
const MAX_OBJECTS_PER_PLATE: usize = 32;
const MAX_FILAMENTS_PER_PLATE: usize = 32;

pub fn parse_artifact_metadata(
    filename: &str,
    content_type: &str,
    path: &Path,
) -> anyhow::Result<Option<ArtifactMetadata>> {
    if !is_3mf_candidate(filename, content_type) {
        return Ok(None);
    }

    let file = File::open(path).with_context(|| {
        format!(
            "failed to open artifact metadata candidate {}",
            redacted_filename(filename)
        )
    })?;
    let archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => return Ok(None),
    };
    parse_zip_archive(filename, archive)
}

fn parse_zip_archive<R: Read + Seek>(
    filename: &str,
    mut archive: ZipArchive<R>,
) -> anyhow::Result<Option<ArtifactMetadata>> {
    let mut draft = Draft::default();
    let mut total_read = 0_u64;
    let entry_count = archive.len().min(MAX_ZIP_ENTRIES);
    if archive.len() > MAX_ZIP_ENTRIES {
        draft.warnings.insert("zip_entry_limit_reached");
    }

    for index in 0..entry_count {
        let mut file = archive
            .by_index(index)
            .context("failed to read 3mf zip entry")?;
        let name = file.name().to_owned();
        if !is_allowed_metadata_name(&name) {
            continue;
        }

        if let Some(plate_id) = plate_id_from_name(&name, ".gcode") {
            draft.ensure_plate(plate_id, PlateSource::Gcode);
            continue;
        }
        if let Some(plate_id) = plate_id_from_name(&name, ".png") {
            if let Some(plate) = draft.ensure_plate(plate_id, PlateSource::Thumbnail) {
                plate.metadata.has_thumbnail = true;
            }
            continue;
        }

        if file.size() > MAX_METADATA_FILE_BYTES {
            draft.warnings.insert("metadata_file_too_large");
            continue;
        }
        if total_read.saturating_add(file.size()) > MAX_TOTAL_METADATA_BYTES {
            draft.warnings.insert("metadata_total_limit_reached");
            continue;
        }

        let mut contents = String::new();
        if file.read_to_string(&mut contents).is_err() {
            continue;
        }
        total_read += contents.len() as u64;

        match name.as_str() {
            "Metadata/slice_info.config" => {
                let _ = parse_slice_info(&contents, &mut draft);
            }
            "Metadata/model_settings.config" => {
                let _ = parse_model_settings(&contents, &mut draft);
            }
            _ => {
                if let Some(plate_id) = plate_id_from_name(&name, ".json") {
                    parse_plate_json(plate_id, &contents, &mut draft);
                }
            }
        }
    }

    if draft.plates.is_empty() {
        return Ok(None);
    }

    let default_plate_id = draft
        .plates
        .values()
        .min_by_key(|draft| (draft.source, draft.metadata.plate_id))
        .map(|draft| draft.metadata.plate_id);
    let plates = draft
        .plates
        .into_values()
        .map(|mut draft| {
            draft.metadata.objects.truncate(MAX_OBJECTS_PER_PLATE);
            draft.metadata.filaments.truncate(MAX_FILAMENTS_PER_PLATE);
            if draft.metadata.name.is_empty() {
                draft.metadata.name = format!("Plate {}", draft.metadata.plate_id);
            }
            draft.metadata
        })
        .collect::<Vec<_>>();
    Ok(Some(ArtifactMetadata {
        source: "bambu_3mf",
        display_name: display_name_from_filename(filename),
        default_plate_id,
        plate_count: plates.len(),
        plates,
        warnings: draft.warnings.into_iter().collect(),
    }))
}

fn is_3mf_candidate(filename: &str, content_type: &str) -> bool {
    let filename = filename.to_ascii_lowercase();
    filename.ends_with(".3mf")
        || filename.ends_with(".gcode.3mf")
        || content_type.eq_ignore_ascii_case("model/3mf")
}

pub(crate) fn display_name_from_filename(filename: &str) -> String {
    let basename = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim();
    let lower = basename.to_ascii_lowercase();
    for suffix in [".gcode.3mf", ".3mf", ".gcode"] {
        if lower.ends_with(suffix) {
            let end = basename.len() - suffix.len();
            let stem = basename[..end].trim();
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    if basename.is_empty() {
        "artifact".to_string()
    } else {
        basename.to_string()
    }
}

fn redacted_filename(filename: &str) -> String {
    filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("artifact")
        .to_string()
}

fn is_allowed_metadata_name(name: &str) -> bool {
    if !name.starts_with("Metadata/") || name.contains("..") || name.starts_with('/') {
        return false;
    }
    matches!(
        name,
        "Metadata/slice_info.config" | "Metadata/model_settings.config"
    ) || plate_id_from_name(name, ".json").is_some()
        || plate_id_from_name(name, ".png").is_some()
        || plate_id_from_name(name, ".gcode").is_some()
}

fn plate_id_from_name(name: &str, suffix: &str) -> Option<u32> {
    let rest = name.strip_prefix("Metadata/plate_")?;
    let value = rest.strip_suffix(suffix)?;
    value.parse::<u32>().ok().filter(|value| *value > 0)
}

fn parse_slice_info(contents: &str, draft: &mut Draft) -> anyhow::Result<()> {
    let mut reader = Reader::from_str(contents);
    let mut current_plate: Option<u32> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => match event.name().as_ref() {
                b"plate" => {
                    let mut plate_id = None;
                    let mut seconds = None;
                    let mut grams = None;
                    for attr in event.attributes().flatten() {
                        let value = attr_value(&reader, &attr)?;
                        match attr.key.as_ref() {
                            b"index" => plate_id = parse_u32(&value),
                            b"prediction" => seconds = parse_u32(&value),
                            b"weight" => grams = parse_f64(&value),
                            _ => {}
                        }
                    }
                    current_plate = plate_id;
                    if let Some(plate_id) = plate_id
                        && let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo)
                    {
                        if plate.metadata.estimated_time_seconds.is_none() {
                            plate.metadata.estimated_time_seconds = seconds;
                        }
                        if plate.metadata.filament_weight_grams.is_none() {
                            plate.metadata.filament_weight_grams = grams;
                        }
                    }
                }
                b"object" => {
                    let Some(plate_id) = current_plate else {
                        continue;
                    };
                    let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo) else {
                        continue;
                    };
                    let mut name = None;
                    for attr in event.attributes().flatten() {
                        if attr.key.as_ref() == b"name" {
                            name = Some(attr_value(&reader, &attr)?);
                        }
                    }
                    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
                        push_object(&mut plate.metadata, name);
                    }
                }
                b"filament" => {
                    let Some(plate_id) = current_plate else {
                        continue;
                    };
                    let Some(plate) = draft.ensure_plate(plate_id, PlateSource::SliceInfo) else {
                        continue;
                    };
                    if plate.metadata.filaments.len() >= MAX_FILAMENTS_PER_PLATE {
                        draft.warnings.insert("filament_limit_reached");
                        continue;
                    }
                    let mut filament = FilamentMetadata {
                        filament_id: None,
                        filament_type: None,
                        color: None,
                        used_grams: None,
                        used_meters: None,
                    };
                    for attr in event.attributes().flatten() {
                        let value = attr_value(&reader, &attr)?;
                        match attr.key.as_ref() {
                            b"id" => filament.filament_id = Some(value),
                            b"type" => filament.filament_type = Some(value),
                            b"color" => filament.color = Some(value),
                            b"used_g" => filament.used_grams = parse_f64(&value),
                            b"used_m" => filament.used_meters = parse_f64(&value),
                            _ => {}
                        }
                    }
                    plate.metadata.filaments.push(filament);
                }
                _ => {}
            },
            Ok(Event::End(event)) if event.name().as_ref() == b"plate" => {
                current_plate = None;
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(err).context("failed to parse slice_info.config"),
            _ => {}
        }
    }
    Ok(())
}

fn parse_model_settings(contents: &str, draft: &mut Draft) -> anyhow::Result<()> {
    let mut reader = Reader::from_str(contents);
    let mut pending_plate_id: Option<u32> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                if event.name().as_ref() != b"metadata" {
                    continue;
                }
                let mut key = None;
                let mut value = None;
                for attr in event.attributes().flatten() {
                    let decoded = attr_value(&reader, &attr)?;
                    match attr.key.as_ref() {
                        b"key" => key = Some(decoded),
                        b"value" => value = Some(decoded),
                        _ => {}
                    }
                }
                match (key.as_deref(), value) {
                    (Some("plater_id"), Some(value)) => {
                        pending_plate_id = parse_u32(&value);
                    }
                    (Some("plater_name"), Some(value)) => {
                        if let Some(plate_id) = pending_plate_id
                            && let Some(plate) =
                                draft.ensure_plate(plate_id, PlateSource::SliceInfo)
                            && !value.trim().is_empty()
                        {
                            plate.metadata.name = value;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(err).context("failed to parse model_settings.config"),
            _ => {}
        }
    }
    Ok(())
}

fn parse_plate_json(plate_id: u32, contents: &str, draft: &mut Draft) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(contents) else {
        return;
    };
    let Some(objects) = value.get("bbox_objects").and_then(|value| value.as_array()) else {
        return;
    };
    let Some(plate) = draft.ensure_plate(plate_id, PlateSource::Json) else {
        return;
    };
    for name in objects.iter().filter_map(|object| {
        object
            .get("name")
            .and_then(|name| name.as_str())
            .filter(|name| !name.trim().is_empty())
    }) {
        push_object(&mut plate.metadata, name.to_string());
    }
}

fn attr_value(reader: &Reader<&[u8]>, attr: &Attribute<'_>) -> quick_xml::Result<String> {
    attr.decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
        .map(|value| value.to_string())
}

fn push_object(plate: &mut PlateMetadata, name: String) {
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

#[cfg(test)]
mod tests;
