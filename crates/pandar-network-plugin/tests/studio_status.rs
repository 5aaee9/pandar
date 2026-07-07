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

#[test]
fn printer_telemetry_defaults_to_studio_safe_idle_shape() {
    let body = telemetry("{}");

    assert!(body.contains(r#""printer_type":"C11""#));
    assert!(body.contains(r#""bed_temper":0"#));
    assert!(body.contains(
        r#""nozzle":{"exist":1,"state":0,"info":[{"id":0,"diameter":0.4,"type":"XS01","stat":0}]}"#
    ));
    assert!(body.contains(r#""extruder":{"state":1,"info":[{"id":0,"info":8,"temp":0,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0}]}"#));
    assert!(body.contains(r#","ams":{"ams":[]}"#));
}

#[test]
fn printer_telemetry_maps_dual_nozzle_temperatures_and_active_tool() {
    let body = telemetry(
        r#"{"dev_model_name":"N6","nozzle_temperatures":[{"label":"L","current_celsius":"28","target_celsius":"220"},{"label":"R","current_celsius":"27","target_celsius":"215"}],"active_nozzle":"L","bed_temperature_celsius":"60","bed_target_temperature_celsius":"65","chamber_temperature_celsius":"32","chamber_light_on":true}"#,
    );

    assert!(body.contains(r#""printer_type":"N6""#));
    assert!(body.contains(r#""nozzle_temper":28"#));
    assert!(body.contains(r#""nozzle_target_temper":220"#));
    assert!(body.contains(r#""nozzle_temper2":27"#));
    assert!(body.contains(r#""nozzle_target_temper2":215"#));
    assert!(body.contains(r#""bed_temp":4259900"#));
    assert!(body.contains(r#""ctc":{"state":1,"info":{"temp":32}}"#));
    assert!(body.contains(r#""nozzle":{"exist":3"#));
    assert!(body.contains(r#""extruder":{"state":18"#));
    assert!(body.contains(r#"{"id":1,"info":8,"temp":14417948,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":1}"#));
    assert!(body.contains(r#"{"id":0,"info":8,"temp":14090267,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0}"#));
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
}
