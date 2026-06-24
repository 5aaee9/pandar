use std::io::Write;

use zip::{ZipWriter, write::SimpleFileOptions};

use super::*;

#[test]
fn gcode_returns_no_metadata() {
    let temp = tempfile::NamedTempFile::new().unwrap();

    let metadata =
        parse_artifact_metadata("plate.gcode", "application/octet-stream", temp.path()).unwrap();

    assert_eq!(metadata, None);
}

#[test]
fn malformed_3mf_returns_no_metadata() {
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    temp.write_all(b"not a zip").unwrap();

    let metadata = parse_artifact_metadata("plate.3mf", "model/3mf", temp.path()).unwrap();

    assert_eq!(metadata, None);
}

#[test]
fn slice_info_extracts_plate_estimates_objects_and_filaments() {
    let temp = zip_fixture(&[(
        "Metadata/slice_info.config",
        r##"
            <config>
              <plate index="2" prediction="3600" weight="18.5">
                <object name="body"/>
                <object name="lid"/>
                <filament id="1" type="PLA" color="#ffffff" used_g="18.5" used_m="6.2"/>
              </plate>
            </config>
            "##,
    )]);

    let metadata = parse_artifact_metadata("../Project File.gcode.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.source, "bambu_3mf");
    assert_eq!(metadata.display_name, "Project File");
    assert_eq!(metadata.default_plate_id, Some(2));
    assert_eq!(metadata.plate_count, 1);
    assert_eq!(metadata.plates[0].plate_id, 2);
    assert_eq!(metadata.plates[0].estimated_time_seconds, Some(3600));
    assert_eq!(metadata.plates[0].filament_weight_grams, Some(18.5));
    assert_eq!(metadata.plates[0].object_count, 2);
    assert_eq!(metadata.plates[0].objects, ["body", "lid"]);
    assert_eq!(
        metadata.plates[0].filaments[0].filament_id.as_deref(),
        Some("1")
    );
    assert_eq!(
        metadata.plates[0].filaments[0].filament_type.as_deref(),
        Some("PLA")
    );
    assert_eq!(
        metadata.plates[0].filaments[0].color.as_deref(),
        Some("#ffffff")
    );
    assert_eq!(metadata.plates[0].filaments[0].used_grams, Some(18.5));
    assert_eq!(metadata.plates[0].filaments[0].used_meters, Some(6.2));
}

#[test]
fn model_settings_extracts_plate_display_name() {
    let temp = zip_fixture(&[
        ("Metadata/plate_3.gcode", ""),
        (
            "Metadata/model_settings.config",
            r#"
            <config>
              <metadata key="plater_id" value="3"/>
              <metadata key="plater_name" value="Engineering Plate"/>
            </config>
            "#,
        ),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.plates[0].name, "Engineering Plate");
}

#[test]
fn plate_json_extracts_fallback_object_names() {
    let temp = zip_fixture(&[(
        "Metadata/plate_4.json",
        r#"{"bbox_objects":[{"name":"bracket"},{"name":"cover"}]}"#,
    )]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.default_plate_id, Some(4));
    assert_eq!(metadata.plates[0].objects, ["bracket", "cover"]);
    assert_eq!(metadata.plates[0].object_count, 2);
}

#[test]
fn default_plate_uses_source_precedence_and_sorted_ids() {
    let temp = zip_fixture(&[
        ("Metadata/plate_9.png", ""),
        (
            "Metadata/plate_7.json",
            r#"{"bbox_objects":[{"name":"json"}]}"#,
        ),
        (
            "Metadata/slice_info.config",
            r#"<config><plate index="5" prediction="1" weight="2"/></config>"#,
        ),
        ("Metadata/plate_3.gcode", ""),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.default_plate_id, Some(3));
    assert_eq!(
        metadata
            .plates
            .iter()
            .map(|plate| plate.plate_id)
            .collect::<Vec<_>>(),
        [3, 5, 7, 9]
    );
}

#[test]
fn default_plate_prefers_gcode_source_over_lower_plate_json() {
    let temp = zip_fixture(&[
        (
            "Metadata/plate_1.json",
            r#"{"bbox_objects":[{"name":"json"}]}"#,
        ),
        ("Metadata/plate_5.gcode", ""),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.default_plate_id, Some(5));
    assert_eq!(
        metadata
            .plates
            .iter()
            .map(|plate| plate.plate_id)
            .collect::<Vec<_>>(),
        [1, 5]
    );
}

#[test]
fn lower_precedence_data_does_not_replace_existing_fields() {
    let temp = zip_fixture(&[
        (
            "Metadata/slice_info.config",
            r#"<config><plate index="2" prediction="42" weight="3"/></config>"#,
        ),
        (
            "Metadata/plate_2.json",
            r#"{"bbox_objects":[{"name":"fallback"}]}"#,
        ),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.plates[0].estimated_time_seconds, Some(42));
    assert_eq!(metadata.plates[0].objects, ["fallback"]);
}

#[test]
fn oversized_metadata_member_returns_partial_warning() {
    let huge = "x".repeat(MAX_METADATA_FILE_BYTES as usize + 1);
    let temp = zip_fixture(&[
        ("Metadata/plate_1.gcode", ""),
        ("Metadata/plate_1.json", &huge),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.default_plate_id, Some(1));
    assert_eq!(metadata.warnings, ["metadata_file_too_large"]);
}

#[test]
fn unknown_and_traversal_entries_are_ignored() {
    let temp = zip_fixture(&[
        ("Metadata/../plate_1.gcode", ""),
        ("Metadata/custom.xml", "<x/>"),
        ("Metadata/plate_2.png", ""),
    ]);

    let metadata = parse_artifact_metadata("part.3mf", "model/3mf", temp.path())
        .unwrap()
        .unwrap();

    assert_eq!(metadata.default_plate_id, Some(2));
    assert!(metadata.plates[0].has_thumbnail);
}

#[test]
fn display_name_strips_known_suffixes() {
    assert_eq!(
        display_name_from_filename("../folder/My Project.gcode.3mf"),
        "My Project"
    );
    assert_eq!(display_name_from_filename("part.3mf"), "part");
    assert_eq!(display_name_from_filename("part.gcode"), "part");
}

fn zip_fixture(entries: &[(&str, &str)]) -> tempfile::NamedTempFile {
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    {
        let mut zip = ZipWriter::new(temp.as_file_mut());
        let options = SimpleFileOptions::default();
        for (name, contents) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    temp
}
