use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::MAX_PLATES;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub source: &'static str,
    pub display_name: String,
    pub default_plate_id: Option<u32>,
    pub plate_count: usize,
    pub plates: Vec<PlateMetadata>,
    pub warnings: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlateMetadata {
    pub plate_id: u32,
    pub name: String,
    pub object_count: usize,
    pub objects: Vec<String>,
    pub estimated_time_seconds: Option<u32>,
    pub filament_weight_grams: Option<f64>,
    pub filaments: Vec<FilamentMetadata>,
    pub has_thumbnail: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentMetadata {
    pub filament_id: Option<String>,
    pub filament_type: Option<String>,
    pub color: Option<String>,
    pub used_grams: Option<f64>,
    pub used_meters: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PlateSource {
    Gcode = 0,
    SliceInfo = 1,
    Json = 2,
    Thumbnail = 3,
}

#[derive(Debug, Clone)]
pub(super) struct PlateDraft {
    pub metadata: PlateMetadata,
    pub source: PlateSource,
}

impl PlateDraft {
    fn new(plate_id: u32, source: PlateSource) -> Self {
        Self {
            metadata: PlateMetadata {
                plate_id,
                name: format!("Plate {plate_id}"),
                ..Default::default()
            },
            source,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Draft {
    pub plates: BTreeMap<u32, PlateDraft>,
    pub warnings: BTreeSet<&'static str>,
}

impl Draft {
    pub fn ensure_plate(&mut self, plate_id: u32, source: PlateSource) -> Option<&mut PlateDraft> {
        if plate_id == 0 {
            return None;
        }
        if !self.plates.contains_key(&plate_id) && self.plates.len() >= MAX_PLATES {
            self.warnings.insert("plate_limit_reached");
            return None;
        }

        let entry = self
            .plates
            .entry(plate_id)
            .or_insert_with(|| PlateDraft::new(plate_id, source));
        if source < entry.source {
            entry.source = source;
        }
        Some(entry)
    }
}
