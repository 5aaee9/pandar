use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct MaterialReport<'a> {
    pub(super) print: MaterialPrint<'a>,
}

#[derive(Default, Serialize)]
pub(super) struct MaterialPrint<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cfg: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) aux: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stat: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_temper: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nozzle_temper2: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) vt_tray: Option<ExternalSource<'a>>,
    pub(super) ams: MaterialAms<'a>,
}

#[derive(Default, Serialize)]
pub(super) struct MaterialAms<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_now: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_exist_bits: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) power_on_flag: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) ams: Vec<MaterialAmsUnit<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) vt_tray: Option<ExternalSource<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) vir_slot: Option<ExternalSource<'a>>,
}

#[derive(Serialize)]
pub(super) struct MaterialAmsUnit<'a> {
    pub(super) id: Scalar<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) info: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) humidity: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) humidity_raw: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temp: Option<&'a str>,
    pub(super) tray: Vec<MaterialTray<'a>>,
}

#[derive(Default, Serialize)]
pub(super) struct MaterialTray<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_info_idx: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_sub_brands: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tag_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_uuid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) k: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) remain: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) cols: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) access_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) passwd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<&'a str>,
}

#[derive(Default, Serialize)]
pub(super) struct ExternalTray<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extruder_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) setting_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_info_idx: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tray_sub_brands: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) remain: Option<Scalar<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ExternalSource<'a> {
    Object(ExternalTray<'a>),
    Array(Vec<ExternalTray<'a>>),
}

#[derive(Clone, Copy, Serialize)]
#[serde(untagged)]
pub(super) enum Scalar<'a> {
    Str(&'a str),
    U32(u32),
}

pub(super) fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
