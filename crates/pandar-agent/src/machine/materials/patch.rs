use pandar_core::AmsUnitKind;
use serde::{Serialize, Serializer};
use serde_json::Number;

#[derive(Serialize)]
pub(crate) struct MaterialPatchDocument<'a> {
    #[serde(rename = "type")]
    pub(super) document_type: &'static str,
    pub(super) observed_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cfg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) aux: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) filament_switch_installed: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) ams_units: Vec<AmsUnitPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) external_spools: Option<Vec<ExternalSpoolPatch>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) replace_external_spools: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) active_tray: Option<ActiveTrayPatch>,
}

#[derive(Serialize)]
pub(super) struct AmsUnitPatch {
    pub(super) unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unit_kind: Option<AmsUnitKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) humidity: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) humidity_level: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature_celsius: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dry_status: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) dry_time_minutes: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) toolhead: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) trays: Vec<MaterialTrayPatch>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) replace_trays: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum MaterialTrayPatch {
    Present(MaterialTrayEntryPatch),
    EmptyClear(EmptyTrayClear),
}

#[derive(Serialize)]
pub(super) struct MaterialTrayEntryPatch {
    pub(super) tray_id: String,
    pub(super) exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unit_kind: Option<AmsUnitKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) global_tray_id: Option<u64>,
    #[serde(flatten)]
    pub(super) fields: MaterialFieldsPatch,
}

#[derive(Serialize)]
pub(super) struct ExternalSpoolPatch {
    pub(super) external_id: String,
    pub(super) exists: bool,
    pub(super) tray_id: String,
    #[serde(flatten)]
    pub(super) fields: MaterialFieldsPatch,
}

#[derive(Default, Serialize)]
pub(super) struct MaterialFieldsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) filament_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) setting_id: Option<String>,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) filament_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) multi_color: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tag_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) remaining_estimate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) k_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) toolhead: Option<String>,
}

#[derive(Serialize)]
pub(super) struct EmptyTrayClear {
    tray_id: String,
    exists: bool,
    unit_kind: AmsUnitKind,
    global_tray_id: Option<u64>,
    state: &'static str,
    filament_id: Option<&'static str>,
    setting_id: Option<&'static str>,
    #[serde(rename = "type")]
    filament_type: Option<&'static str>,
    color: Option<&'static str>,
    multi_color: Option<Vec<String>>,
    tag_uid: Option<&'static str>,
    tray_uuid: Option<&'static str>,
    name: Option<&'static str>,
    remaining_estimate: Option<&'static str>,
}

pub(super) enum ActiveTrayPatch {
    None,
    External(ExternalActiveTray),
    Ams(AmsActiveTray),
    AmsHt(AmsHtActiveTray),
}

#[derive(Serialize)]
pub(super) struct ExternalActiveTray {
    kind: &'static str,
    external_id: &'static str,
    tray_id: &'static str,
    global_tray_id: Option<u64>,
}

#[derive(Serialize)]
pub(super) struct AmsActiveTray {
    kind: &'static str,
    global_tray_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    ams_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_id: Option<String>,
}

#[derive(Serialize)]
pub(super) struct AmsHtActiveTray {
    kind: &'static str,
    global_tray_id: Option<u64>,
    ams_id: String,
    tray_id: &'static str,
}

pub(super) fn empty_tray_clear_patch(
    tray_id: String,
    unit_kind: AmsUnitKind,
    global_tray_id: Option<u64>,
) -> MaterialTrayPatch {
    MaterialTrayPatch::EmptyClear(EmptyTrayClear {
        tray_id,
        exists: false,
        unit_kind,
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
}

pub(super) fn external_active_tray_patch() -> ActiveTrayPatch {
    ActiveTrayPatch::External(ExternalActiveTray {
        kind: "external",
        external_id: "254",
        tray_id: "0",
        global_tray_id: None,
    })
}

pub(super) fn ams_active_tray_patch(
    tray_now: i64,
    ams_id: String,
    tray_id: String,
) -> ActiveTrayPatch {
    ActiveTrayPatch::Ams(AmsActiveTray {
        kind: "ams",
        global_tray_id: tray_now,
        ams_id: Some(ams_id),
        tray_id: Some(tray_id),
    })
}

pub(super) fn mixed_ams_lite_global_active_tray_patch(tray_now: i64) -> ActiveTrayPatch {
    ActiveTrayPatch::Ams(AmsActiveTray {
        kind: "ams",
        global_tray_id: tray_now,
        ams_id: None,
        tray_id: None,
    })
}

pub(super) fn ams_ht_active_tray_patch(tray_now: i64) -> ActiveTrayPatch {
    ActiveTrayPatch::AmsHt(AmsHtActiveTray {
        kind: "ams_ht",
        global_tray_id: None,
        ams_id: tray_now.to_string(),
        tray_id: "0",
    })
}

impl Serialize for ActiveTrayPatch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::None => serializer.serialize_none(),
            Self::External(value) => value.serialize(serializer),
            Self::Ams(value) => value.serialize(serializer),
            Self::AmsHt(value) => value.serialize(serializer),
        }
    }
}
