#[derive(Clone, Copy)]
pub(super) enum Expectation {
    Wire(&'static str, &'static str),
    WireJson(&'static str, &'static str),
    Admitted,
    ArtifactSource,
    ConfigSource,
    ScrubValue(&'static str),
    ScrubField(&'static str),
    Reject,
    Unsupported,
}

pub(super) type FieldCase = (&'static str, &'static str, Expectation);

pub(super) const FIELD_CASES: [FieldCase; 45] = [
    (
        "dev_id",
        "preserve",
        Expectation::Wire("printer_id", "printer-1"),
    ),
    (
        "task_name",
        "preserve",
        Expectation::Wire("task_name", "task-sentinel.3mf"),
    ),
    (
        "project_name",
        "preserve",
        Expectation::Wire("project_name", "project-sentinel"),
    ),
    (
        "preset_name",
        "preserve",
        Expectation::Wire("preset_name", "preset-sentinel"),
    ),
    ("filename", "preserve", Expectation::ArtifactSource),
    ("config_filename", "preserve", Expectation::ConfigSource),
    (
        "plate_index",
        "preserve",
        Expectation::Wire("plate_id", "713"),
    ),
    (
        "ftp_folder",
        "default",
        Expectation::ScrubValue("/contract/private/ftp-folder"),
    ),
    ("ftp_file", "reject", Expectation::Reject),
    ("ftp_file_md5", "reject", Expectation::Reject),
    (
        "nozzle_mapping",
        "preserve",
        Expectation::WireJson("nozzle_mapping", "[1,0]"),
    ),
    (
        "ams_mapping",
        "preserve",
        Expectation::WireJson("ams_mapping", "[17,23]"),
    ),
    (
        "ams_mapping2",
        "preserve",
        Expectation::WireJson("ams_mapping2", r#"[{"ams_id":17,"slot_id":23}]"#),
    ),
    (
        "ams_mapping_info",
        "preserve",
        Expectation::WireJson(
            "ams_mapping_info",
            r#"[{"ams":17,"targetColor":"11223344","filamentId":"GFA00","filamentType":"PLA","nozzleId":0,"sourceColor":"55667788"}]"#,
        ),
    ),
    (
        "nozzles_info",
        "preserve",
        Expectation::WireJson(
            "nozzles_info",
            r#"[{"id":0,"type":null,"flowSize":"H","diameter":0.4},{"id":1,"type":null,"flowSize":"S","diameter":0.6}]"#,
        ),
    ),
    ("connection_type", "preserve", Expectation::Admitted),
    (
        "comments",
        "preserve",
        Expectation::Wire("comments", "comment-sentinel"),
    ),
    (
        "origin_profile_id",
        "preserve",
        Expectation::Wire("origin_profile_id", "29"),
    ),
    (
        "stl_design_id",
        "preserve",
        Expectation::Wire("stl_design_id", "31"),
    ),
    (
        "origin_model_id",
        "preserve",
        Expectation::Wire("origin_model_id", "model-sentinel"),
    ),
    ("print_type", "preserve", Expectation::Admitted),
    ("dst_file", "unsupported", Expectation::Unsupported),
    (
        "dev_name",
        "preserve",
        Expectation::Wire("dev_name", "device-name-sentinel"),
    ),
    (
        "dev_ip",
        "default",
        Expectation::ScrubValue("198.51.100.77"),
    ),
    (
        "use_ssl_for_ftp",
        "default",
        Expectation::ScrubField("use_ssl_for_ftp"),
    ),
    (
        "use_ssl_for_mqtt",
        "default",
        Expectation::ScrubField("use_ssl_for_mqtt"),
    ),
    (
        "username",
        "default",
        Expectation::ScrubValue("username-secret-sentinel"),
    ),
    (
        "password",
        "default",
        Expectation::ScrubValue("password-secret-sentinel"),
    ),
    (
        "task_bed_leveling",
        "preserve",
        Expectation::Wire("bed_leveling", "true"),
    ),
    (
        "task_flow_cali",
        "preserve",
        Expectation::Wire("flow_cali", "true"),
    ),
    (
        "task_vibration_cali",
        "preserve",
        Expectation::Wire("vibration_cali", "true"),
    ),
    (
        "task_layer_inspect",
        "preserve",
        Expectation::Wire("layer_inspect", "true"),
    ),
    (
        "task_record_timelapse",
        "preserve",
        Expectation::Wire("timelapse", "true"),
    ),
    (
        "task_timelapse_use_internal",
        "preserve",
        Expectation::Wire("timelapse_use_internal", "true"),
    ),
    (
        "task_use_ams",
        "preserve",
        Expectation::Wire("use_ams", "true"),
    ),
    (
        "task_bed_type",
        "preserve",
        Expectation::Wire("bed_type", "supertack_plate"),
    ),
    ("extra_options", "reject", Expectation::Reject),
    (
        "auto_bed_leveling",
        "preserve",
        Expectation::Wire("auto_bed_leveling", "2"),
    ),
    (
        "auto_flow_cali",
        "preserve",
        Expectation::Wire("auto_flow_cali", "2"),
    ),
    (
        "auto_offset_cali",
        "preserve",
        Expectation::Wire("auto_offset_cali", "2"),
    ),
    (
        "extruder_cali_manual_mode",
        "preserve",
        Expectation::Wire("extruder_cali_manual_mode", "1"),
    ),
    (
        "task_ext_change_assist",
        "unsupported",
        Expectation::Unsupported,
    ),
    (
        "try_emmc_print",
        "preserve",
        Expectation::Wire("try_emmc_print", "true"),
    ),
    (
        "svc_context",
        "preserve",
        Expectation::Wire("svc_context", "service-context-sentinel"),
    ),
    (
        "slicer_uid",
        "preserve",
        Expectation::Wire("slicer_uid", "slicer-uid-sentinel"),
    ),
];
