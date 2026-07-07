use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct MaterialPatchDocument<'a> {
    #[serde(rename = "type")]
    pub(super) document_type: &'static str,
    pub(super) observed_at: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) ams_units: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) external_spools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) replace_external_spools: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) active_tray: Option<Value>,
}

#[derive(Serialize)]
struct EmptyTrayClear<'a> {
    tray_id: String,
    exists: bool,
    unit_kind: &'static str,
    global_tray_id: Option<u64>,
    state: &'static str,
    filament_id: Option<&'a str>,
    setting_id: Option<&'a str>,
    #[serde(rename = "type")]
    filament_type: Option<&'a str>,
    color: Option<&'a str>,
    multi_color: Option<Vec<Value>>,
    tag_uid: Option<&'a str>,
    tray_uuid: Option<&'a str>,
    name: Option<&'a str>,
    remaining_estimate: Option<&'a str>,
}

#[derive(Serialize)]
struct ExternalActiveTray<'a> {
    kind: &'static str,
    external_id: &'a str,
    tray_id: &'a str,
    global_tray_id: Option<u64>,
}

#[derive(Serialize)]
struct AmsActiveTray {
    kind: &'static str,
    global_tray_id: i64,
    ams_id: String,
    tray_id: String,
}

#[derive(Serialize)]
struct AmsHtActiveTray {
    kind: &'static str,
    global_tray_id: Option<u64>,
    ams_id: String,
    tray_id: &'static str,
}

pub(super) fn empty_tray_clear_value(tray_id: String, global_tray_id: Option<u64>) -> Value {
    serde_json::to_value(EmptyTrayClear {
        tray_id,
        exists: false,
        unit_kind: "ams",
        global_tray_id,
        state: "9",
        filament_id: None,
        setting_id: None,
        filament_type: None,
        color: None,
        multi_color: None,
        tag_uid: None,
        tray_uuid: None,
        name: None,
        remaining_estimate: None,
    })
    .expect("empty tray clear is serializable")
}

pub(super) fn external_active_tray_value() -> Value {
    serde_json::to_value(ExternalActiveTray {
        kind: "external",
        external_id: "254",
        tray_id: "0",
        global_tray_id: None,
    })
    .expect("external active tray is serializable")
}

pub(super) fn ams_active_tray_value(tray_now: i64) -> Value {
    serde_json::to_value(AmsActiveTray {
        kind: "ams",
        global_tray_id: tray_now,
        ams_id: (tray_now / 4).to_string(),
        tray_id: (tray_now % 4).to_string(),
    })
    .expect("AMS active tray is serializable")
}

pub(super) fn ams_ht_active_tray_value(tray_now: i64) -> Value {
    serde_json::to_value(AmsHtActiveTray {
        kind: "ams_ht",
        global_tray_id: None,
        ams_id: tray_now.to_string(),
        tray_id: "0",
    })
    .expect("AMS HT active tray is serializable")
}
