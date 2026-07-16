use pandar_network_plugin::{PluginHttpResult, pandar_plugin_free_with_capacity};

unsafe extern "C" {
    fn pandar_plugin_printer_telemetry_json(
        printer_ptr: *const u8,
        printer_len: usize,
    ) -> PluginHttpResult;
}

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn telemetry(printer: &str) -> String {
    let result = unsafe { pandar_plugin_printer_telemetry_json(printer.as_ptr(), printer.len()) };
    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    body(result)
}

fn telemetry_json(printer: &str) -> serde_json::Value {
    serde_json::from_str(&format!("{{{}}}", telemetry(printer))).unwrap()
}

#[test]
fn studio_status_emits_native_print_error_and_job_id() {
    let telemetry = telemetry_json(r#"{"print_error":83918929,"job_id":"job-7"}"#);

    assert_eq!(telemetry["print_error"], serde_json::json!(83_918_929));
    assert_eq!(telemetry["job_id"], serde_json::json!("job-7"));
}

#[test]
fn studio_status_preserves_explicit_clear_and_empty_job_id() {
    let telemetry = telemetry_json(r#"{"print_error":0,"job_id":""}"#);

    assert_eq!(telemetry["print_error"], serde_json::json!(0));
    assert_eq!(telemetry["job_id"], serde_json::json!(""));
}

#[test]
fn studio_status_omits_unknown_native_error_fields() {
    let telemetry = telemetry_json("{}");

    assert!(telemetry.get("print_error").is_none());
    assert!(telemetry.get("job_id").is_none());
}

#[test]
fn studio_status_preserves_fun_bitmap_exactly() {
    let telemetry = telemetry_json(r#"{"fun":"8000004100000020"}"#);

    assert_eq!(telemetry["fun"], serde_json::json!("8000004100000020"));
}

#[test]
fn studio_status_defaults_missing_or_null_fun_without_discarding_telemetry() {
    for fun in ["", r#","fun":null"#] {
        let telemetry = telemetry_json(&format!(
            r#"{{"gcode_state":"RUNNING","mc_percent":37,"bed_temperature_celsius":"60","hms":[{{"attr":134152704,"code":32785}}],"materials":{{"ams_units":[{{"unit_id":"0","trays":[{{"tray_id":"0","type":"PETG-CF"}}]}}]}}{fun}}}"#
        ));

        assert_eq!(telemetry["fun"], serde_json::json!("0"));
        assert_eq!(telemetry["gcode_state"], serde_json::json!("RUNNING"));
        assert_eq!(telemetry["mc_percent"], serde_json::json!(37));
        assert_eq!(telemetry["bed_temper"], serde_json::json!(60));
        assert_eq!(telemetry["hms"][0]["attr"], serde_json::json!(134_152_704));
        assert_eq!(telemetry["hms"][0]["code"], serde_json::json!(32_785));
        assert_eq!(
            telemetry["ams"]["ams"][0]["tray"][0]["tray_type"],
            serde_json::json!("PETG-CF")
        );
    }
}

#[test]
fn printer_telemetry_defaults_to_studio_safe_idle_shape() {
    let body = telemetry("{}");

    assert!(body.contains(r#""gcode_state":"IDLE""#));
    assert!(body.contains(r#""mc_percent":0"#));
    assert!(body.contains(r#""mc_remaining_time":0"#));
    assert!(body.contains(r#""layer_num":0"#));
    assert!(body.contains(r#""total_layer_num":0"#));
    assert!(body.contains(r#""project_id":"0""#));
    assert!(body.contains(r#""profile_id":"0""#));
    assert!(body.contains(r#""subtask_id":"0""#));
    assert!(body.contains(r#""hms":[]"#));
    assert!(body.contains(r#""printer_type":"C11""#));
    assert!(body.contains(r#""aux":"""#));
    assert!(body.contains(r#""bed_temper":0"#));
    assert!(body.contains(
        r#""nozzle":{"exist":1,"state":0,"info":[{"id":0,"diameter":0.4,"type":"XS01","stat":0}]}"#
    ));
    assert!(body.contains(r#""extruder":{"state":1,"info":[{"id":0,"filam_bak":[],"info":8,"temp":0,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0}]}"#));
    assert!(body.contains(r#","ams":{"ams":[]}"#));
}

#[test]
fn printer_telemetry_maps_dual_nozzle_temperatures_and_active_tool() {
    let body = telemetry(
        r#"{"dev_model_name":"N6","nozzle_temperatures":[{"label":"L","current_celsius":"28","target_celsius":"220","diameter_mm":"0.4","nozzle_type":"XH05"},{"label":"R","current_celsius":"27","target_celsius":"215","diameter_mm":"0.6","nozzle_type":"XS01"}],"active_nozzle":"L","bed_temperature_celsius":"60","bed_target_temperature_celsius":"65","chamber_temperature_celsius":"32","chamber_light_on":true}"#,
    );

    assert!(body.contains(r#""printer_type":"N6""#));
    assert!(body.contains(r#""nozzle_temper":27"#));
    assert!(body.contains(r#""nozzle_target_temper":215"#));
    assert!(body.contains(r#""nozzle_temper2":28"#));
    assert!(body.contains(r#""nozzle_target_temper2":220"#));
    assert!(body.contains(r#""nozzle_type":"XS01""#));
    assert!(body.contains(r#""nozzle_diameter":0.6"#));
    assert!(body.contains(r#""nozzle_type2":"XH05""#));
    assert!(body.contains(r#""nozzle_diameter2":0.4"#));
    assert!(body.contains(r#""bed_temp":4259900"#));
    assert!(body.contains(r#""ctc":{"state":1,"info":{"temp":32}}"#));
    assert!(body.contains(r#""nozzle":{"exist":3"#));
    assert!(body.contains(r#"{"id":0,"diameter":0.6,"type":"XS01","stat":0}"#));
    assert!(body.contains(r#"{"id":1,"diameter":0.4,"type":"XH05","stat":0}"#));
    assert!(body.contains(r#"{"id":0,"diameter":0.6,"type":"XS01","stat":0},{"id":1,"diameter":0.4,"type":"XH05","stat":0}"#));
    assert!(body.contains(r#""extruder":{"state":18"#));
    assert!(body.contains(r#"{"id":1,"filam_bak":[],"info":8,"temp":14417948,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":1}"#));
    assert!(body.contains(r#"{"id":0,"filam_bak":[],"info":8,"temp":14090267,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0},{"id":1,"filam_bak":[],"info":8,"temp":14417948,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":1}"#));
    assert!(body.contains(r#""lights_report":[{"node":"chamber_light","mode":"on"}]"#));
}

#[test]
fn printer_telemetry_maps_ams_and_external_materials() {
    let body = telemetry(
        r##"{"materials":{"ams_units":[{"unit_id":"0","humidity":25,"humidity_level":3,"temperature_celsius":28.5,"toolhead":"R","trays":[{"tray_id":"0","global_tray_id":0,"type":"PETG-CF","filament_id":"GFG50","color":"000000FF","remaining_estimate":"-1"},{"tray_id":"1","global_tray_id":1,"type":"PLA","filament_id":"GFA00","color":"C12E1FFF","remaining_estimate":"100"}]},{"unit_id":"1","humidity":28,"humidity_level":3,"temperature_celsius":28.1,"toolhead":"L","trays":[{"tray_id":"0","global_tray_id":4,"type":"PLA","filament_id":"GFA00","color":"000000FF","remaining_estimate":"55"}]}],"external_spools":[{"external_id":"254","tray_id":"0","type":"PETG","filament_id":"GFG00","color":"11223344","toolhead":"L"},{"external_id":"255","tray_id":"1","type":"PLA","filament_id":"GFL99","color":"46A8F9FF","toolhead":"R"}],"active_tray":{"kind":"ams","ams_id":"0","tray_id":"1","global_tray_id":1}}}"##,
    );

    assert!(body.contains(r#""ams_exist_bits":"3""#));
    assert!(body.contains(r#""tray_exist_bits":"13""#));
    assert!(body.contains(r#""tray_now":"1""#));
    assert!(body.contains(r#""id":"0","info":"1""#));
    assert!(body.contains(r#""id":"1","info":"101""#));
    assert!(body.contains(r#""humidity":"3""#));
    assert!(body.contains(r#""humidity_raw":"25""#));
    assert!(body.contains(r#""temp":"28.5""#));
    assert!(body.contains(r#""tray_info_idx":"GFG50""#));
    assert!(body.contains(r#""tray_type":"PETG-CF""#));
    assert!(body.contains(r#""remain":-1"#));
    assert!(body.contains(r#""vir_slot":[{"id":"254""#));
    assert!(body.contains(r#""id":"255""#));
    assert!(body.contains(r#""tray_type":"PETG""#));
    assert!(body.contains(r#""aux":"""#));
}

#[test]
fn printer_telemetry_preserves_filament_switch_aux_and_routes() {
    let telemetry = telemetry_json(
        r#"{"materials":{"filament_switch_installed":true,"ams_units":[{"unit_id":"0","info":"00000E00","toolhead":"LR","trays":[]},{"unit_id":"1","info":"01000E00","toolhead":"LR","trays":[]}]}}"#,
    );

    assert_eq!(telemetry["aux"], serde_json::json!("20000000"));
    assert_eq!(telemetry["ams"]["ams_exist_bits"], serde_json::json!("3"));
    assert_eq!(telemetry["ams"]["ams"][0]["id"], serde_json::json!("0"));
    assert_eq!(
        telemetry["ams"]["ams"][0]["info"],
        serde_json::json!("00000E00")
    );
    assert_eq!(telemetry["ams"]["ams"][1]["id"], serde_json::json!("1"));
    assert_eq!(
        telemetry["ams"]["ams"][1]["info"],
        serde_json::json!("01000E00")
    );
}

#[test]
fn printer_telemetry_keeps_legacy_lr_projection_when_switch_is_absent() {
    let telemetry = telemetry_json(
        r#"{"materials":{"filament_switch_installed":false,"ams_units":[{"unit_id":"0","info":"00000E00","toolhead":"R","trays":[]},{"unit_id":"1","info":"01000E00","toolhead":"L","trays":[]}]}}"#,
    );

    assert_eq!(telemetry["aux"], serde_json::json!("00000000"));
    assert_eq!(telemetry["ams"]["ams"][0]["info"], serde_json::json!("1"));
    assert_eq!(telemetry["ams"]["ams"][1]["info"], serde_json::json!("101"));
}

#[test]
fn printer_telemetry_drops_invalid_filament_switch_routes() {
    let telemetry = telemetry_json(
        r#"{"materials":{"filament_switch_installed":true,"ams_units":[{"unit_id":"0","info":"00000E00","toolhead":"LR","trays":[{"tray_id":"0"}]},{"unit_id":"1","toolhead":"LR","trays":[{"tray_id":"0"}]},{"unit_id":"2","info":"00000000","toolhead":"LR","trays":[{"tray_id":"0"}]},{"unit_id":"3","info":"02000E00","toolhead":"LR","trays":[{"tray_id":"0"}]},{"unit_id":"4","info":"not-hex","toolhead":"LR","trays":[{"tray_id":"0"}]}]}}"#,
    );

    assert_eq!(telemetry["aux"], serde_json::json!("20000000"));
    assert_eq!(telemetry["ams"]["ams_exist_bits"], serde_json::json!("1"));
    assert_eq!(telemetry["ams"]["tray_exist_bits"], serde_json::json!("1"));
    assert_eq!(telemetry["ams"]["ams"].as_array().unwrap().len(), 1);
    assert_eq!(telemetry["ams"]["ams"][0]["id"], serde_json::json!("0"));
}

#[test]
fn printer_telemetry_maps_live_print_progress_and_hms() {
    let body = telemetry(
        r#"{"gcode_state":"RUNNING","mc_percent":37,"mc_remaining_time":52,"layer_num":12,"total_layer_num":120,"task_id":"task-42","subtask_id":"subtask-42","gcode_file":"drawer-organizer.gcode","subtask_name":"drawer-organizer","hms":[{"attr":134152704,"code":32785}]}"#,
    );

    assert!(body.contains(r#""gcode_state":"RUNNING""#));
    assert!(body.contains(r#""mc_percent":37"#));
    assert!(body.contains(r#""mc_remaining_time":52"#));
    assert!(body.contains(r#""layer_num":12"#));
    assert!(body.contains(r#""total_layer_num":120"#));
    assert!(body.contains(r#""task_id":"task-42""#));
    assert!(body.contains(r#""project_id":"0""#));
    assert!(body.contains(r#""profile_id":"0""#));
    assert!(body.contains(r#""subtask_id":"subtask-42""#));
    assert!(body.contains(r#""gcode_file":"drawer-organizer.gcode""#));
    assert!(body.contains(r#""subtask_name":"drawer-organizer""#));
    assert!(body.contains(r#""hms":[{"attr":134152704,"code":32785}]"#));
}

#[test]
fn nullable_chamber_light_does_not_discard_live_print_status() {
    let body = telemetry(
        r#"{"gcode_state":"RUNNING","mc_percent":37,"hms":[{"attr":134152704,"code":32785}],"chamber_light_on":null}"#,
    );

    assert!(body.contains(r#""gcode_state":"RUNNING""#));
    assert!(body.contains(r#""mc_percent":37"#));
    assert!(body.contains(r#""hms":[{"attr":134152704,"code":32785}]"#));
    assert!(body.contains(r#""lights_report":[{"node":"chamber_light","mode":"off"}]"#));
}
