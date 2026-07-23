pub(crate) use std::time::{Duration, Instant};

pub(crate) use pandar_core::{FirmwareCatalogEntry, FirmwareCatalogTarget};
pub(crate) use pandar_network_plugin::firmware::{FirmwareStatusCache, firmware_catalog_json};

pub(crate) fn populated_firmware() -> serde_json::Value {
    serde_json::json!({
        "session_id": "session-1",
        "generation": 5,
        "module_revision": 8,
        "status_revision": 9,
        "modules": [
            {
                "name": "ota", "sw_ver": "01.02.03.04", "sw_new_ver": "01.02.04.00",
                "new_ver": "01.02.05.00", "visible": false, "product_name": "Printer",
                "sn": "SERIAL", "hw_ver": "AP05", "flag": 5
            },
            {"name": "n3s/0", "sw_ver": "00.00.01.00"},
            {"name": "n3s/0", "sw_ver": "00.00.02.00"}
        ],
        "upgrade_state": populated_upgrade_state(),
        "cfg": "101"
    })
}

pub(crate) fn marker_firmware(session_id: &str, generation: u64) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "generation": generation,
        "module_revision": 0,
        "status_revision": 0
    })
}

pub(crate) fn status_json(cache: &mut FirmwareStatusCache, now: Instant) -> serde_json::Value {
    serde_json::from_str(
        &cache
            .next_status_override_at("SERIAL", now)
            .expect("firmware status override"),
    )
    .unwrap()
}

pub(crate) fn populated_upgrade_state() -> serde_json::Value {
    serde_json::json!({
        "status": "UPGRADING", "progress": "37", "message": "flashing", "module": "ota",
        "err_code": 12, "new_version_state": 2, "consistency_request": true,
        "force_upgrade": true, "dis_state": 3, "ota_new_version_number": "01.02.04.00",
        "ams_new_version_number": "02.00.00.00", "ahb_new_version_number": "03.00.00.00",
        "new_ver_list": [{"name":"ota","cur_ver":"1","new_ver":"2"}],
        "mc_for_ams_firmware": {
            "firmware": [{"id":4,"name":"stable","version":"02.00.00.00"}],
            "current_firmware_id": 4, "current_run_firmware_id": 3, "status": "SWITCHING"
        }
    })
}

pub(crate) fn exact_reset() -> serde_json::Value {
    serde_json::json!({
        "info":{"command":"get_version","sequence_id":"0","result":"fail","module":[]},
        "print":{"command":"push_status","msg":0,"cfg":"","upgrade_state":{
            "status":"","progress":"","message":"","module":"","err_code":0,
            "new_version_state":0,"consistency_request":false,"force_upgrade":false,"dis_state":0,
            "ota_new_version_number":"","ams_new_version_number":"","ahb_new_version_number":"",
            "new_ver_list":[],"mc_for_ams_firmware":{"firmware":[],"current_firmware_id":-1,
            "current_run_firmware_id":-1,"status":""}
        }}
    })
}

pub(crate) fn batch_json(firmware: Option<serde_json::Value>) -> String {
    batch_json_value(firmware).to_string()
}

pub(crate) fn batch_json_value(firmware: Option<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"message":"success","devices":[{
        "dev_id":"SERIAL","dev_name":"Printer","name":"Printer",
        "dev_model_name":"N6","model":"N6","dev_online":true,
        "online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE",
        "mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":null,
        "print_error":null,"job_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,
        "hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,
        "bed_temperature_celsius":null,"bed_target_temperature_celsius":null,
        "chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null,
        "firmware":firmware
    }]})
}
