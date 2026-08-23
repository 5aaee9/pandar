pub(super) const PRINTERS_RESPONSE: &str = r##"{"message":"success","devices":[{"dev_id":"studio-serial-1","fun":"8001004100000020","dev_name":"Probe Printer","pandar_printer_id":"printer-1","name":"Probe Printer","dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"RUNNING","state":"RUNNING","gcode_state":"RUNNING","mc_percent":37,"mc_remaining_time":52,"layer_num":12,"total_layer_num":120,"task_id":"task-42","subtask_id":"subtask-42","gcode_file":"drawer-organizer.gcode","subtask_name":"drawer-organizer","hms":[{"attr":134152704,"code":32785}],"nozzle_temperatures":[{"label":"L","current_celsius":"28","target_celsius":"220","diameter_mm":"0.4","nozzle_type":"HH05"},{"label":"R","current_celsius":"27","target_celsius":"215","diameter_mm":"0.4","nozzle_type":"HS01"}],"active_nozzle":"L","bed_temperature_celsius":"60","bed_target_temperature_celsius":"65","chamber_temperature_celsius":"32","chamber_light_on":true,"materials":{"ams_units":[{"unit_id":"0","humidity":25,"humidity_level":3,"temperature_celsius":28.5,"toolhead":"R","trays":[{"tray_id":"0","global_tray_id":0,"type":"PETG-CF","filament_id":"GFG50","color":"000000FF","remaining_estimate":"-1"},{"tray_id":"1","global_tray_id":1,"type":"PLA","filament_id":"GFA00","color":"C12E1FFF","remaining_estimate":"100"},{"tray_id":"2","global_tray_id":2,"type":"PETG","filament_id":"GFG00","color":"FCE300FF","remaining_estimate":"36"},{"tray_id":"3","global_tray_id":3,"type":"PLA","filament_id":"GFL99","color":"FFF144FF","remaining_estimate":"-1"}]},{"unit_id":"1","humidity":28,"humidity_level":3,"temperature_celsius":28.1,"toolhead":"L","trays":[{"tray_id":"0","global_tray_id":4,"type":"PLA","filament_id":"GFA00","color":"000000FF","remaining_estimate":"55"},{"tray_id":"1","global_tray_id":5,"type":"ABS","filament_id":"GFB00","color":"46A8F9FF","remaining_estimate":"-1"},{"tray_id":"2","global_tray_id":6,"type":"ABS","filament_id":"GFB00","color":"057748FF","remaining_estimate":"-1"},{"tray_id":"3","global_tray_id":7,"type":"PLA-CF","filament_id":"GFA50","color":"69398EFF","remaining_estimate":"85"}]}],"external_spools":[{"external_id":"254","tray_id":"0","type":"PETG","filament_id":"GFG00","color":"11223344","toolhead":"L"},{"external_id":"255","tray_id":"1","type":"PLA","filament_id":"GFL99","color":"46A8F9FF","toolhead":"R"}],"active_tray":{"kind":"ams","ams_id":"0","tray_id":"3","global_tray_id":3},"observed_at":"2026-06-20T00:01:00Z"}}]}"##;

pub(super) fn filament_switch_printers_response() -> String {
    let mut response: serde_json::Value = serde_json::from_str(PRINTERS_RESPONSE).unwrap();
    let materials = &mut response["devices"][0]["materials"];
    materials["filament_switch_installed"] = serde_json::json!(true);
    materials["cfg"] = serde_json::json!("8000000000000001");
    materials["aux"] = serde_json::json!("A4003001");
    materials["stat"] = serde_json::json!("1000000001");
    let ams_units = materials["ams_units"].as_array_mut().unwrap();
    ams_units[0]["info"] = serde_json::json!("00000E00");
    ams_units[0]["toolhead"] = serde_json::json!("LR");
    ams_units[1]["info"] = serde_json::json!("01000E00");
    ams_units[1]["toolhead"] = serde_json::json!("LR");
    response.to_string()
}

pub(super) fn printers_response_with_progress(progress: u8) -> String {
    filament_switch_printers_response().replacen(
        r#""mc_percent":37"#,
        &format!(r#""mc_percent":{progress}"#),
        1,
    )
}

pub(super) fn axis_printers_response() -> String {
    let mut response: serde_json::Value =
        serde_json::from_str(&filament_switch_printers_response()).unwrap();
    let devices = response["devices"].as_array_mut().unwrap();
    let mut second = devices[0].clone();
    second["dev_id"] = serde_json::json!("studio-serial-2");
    second["pandar_printer_id"] = serde_json::json!("printer-2");
    second["dev_name"] = serde_json::json!("Probe Printer 2");
    second["name"] = serde_json::json!("Probe Printer 2");
    devices.push(second);
    response.to_string()
}

/// Converts a retired plugin printer-list response body into printer-events
/// stream frames: a complete snapshot carrying the same device records.
pub(super) fn snapshot_frames(printers_response: &str) -> Vec<String> {
    let mut frames = vec![r#"{"type":"snapshot_begin","version":1}"#.to_owned()];
    let response: serde_json::Value = serde_json::from_str(printers_response).unwrap();
    for device in response["devices"].as_array().unwrap() {
        frames.push(format!(
            r#"{{"type":"printer_upsert","printer":{}}}"#,
            device
        ));
    }
    frames.push(r#"{"type":"snapshot_end","version":1}"#.to_owned());
    frames
}

pub(super) fn camera_printers_response() -> String {
    let mut response: serde_json::Value = serde_json::from_str(PRINTERS_RESPONSE).unwrap();
    let template = response["devices"][0].clone();
    response["devices"] = serde_json::Value::Array(
        [
            ("studio-camera-a1", "printer-camera-a1", "N2S"),
            ("studio-camera-a1-mini", "printer-camera-a1-mini", "N1"),
            ("studio-camera-p1s", "printer-camera-p1s", "C12"),
            ("studio-camera-a2l", "printer-camera-a2l", "N9"),
        ]
        .into_iter()
        .map(|(dev_id, printer_id, model)| {
            let mut device = template.clone();
            device["dev_id"] = serde_json::json!(dev_id);
            device["pandar_printer_id"] = serde_json::json!(printer_id);
            device["dev_model_name"] = serde_json::json!(model);
            device["model"] = serde_json::json!(model);
            device["studio_local_camera"] = serde_json::json!(true);
            device
        })
        .collect(),
    );
    response.to_string()
}
