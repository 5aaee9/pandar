use serde::Serialize;
use serde_json::Value;

use super::input::*;
use super::{single_unit_report, unit_with_tray};

#[derive(Serialize)]
struct EmptyReport {}

#[derive(Serialize)]
struct NullAmsReport {
    print: NullAmsPrint,
}

#[derive(Serialize)]
struct NullAmsPrint {
    ams: Option<()>,
}

#[derive(Serialize)]
struct UnitWithoutTrayReport<'a> {
    print: UnitWithoutTrayPrint<'a>,
}

#[derive(Serialize)]
struct UnitWithoutTrayPrint<'a> {
    ams: UnitWithoutTrayAms<'a>,
}

#[derive(Serialize)]
struct UnitWithoutTrayAms<'a> {
    ams: Vec<UnitWithoutTray<'a>>,
}

#[derive(Serialize)]
struct UnitWithoutTray<'a> {
    id: Scalar<'a>,
}

pub(crate) fn empty_report() -> Value {
    value(EmptyReport {})
}

pub(crate) fn null_ams_report() -> Value {
    value(NullAmsReport {
        print: NullAmsPrint { ams: None },
    })
}

pub(crate) fn empty_ams_unit_report() -> Value {
    value(UnitWithoutTrayReport {
        print: UnitWithoutTrayPrint {
            ams: UnitWithoutTrayAms {
                ams: vec![UnitWithoutTray { id: Scalar::U32(0) }],
            },
        },
    })
}

pub(crate) fn external_spool_single_object_report() -> Value {
    external_source_report(ExternalSource::Object(ExternalTray {
        tray_type: Some("PLA"),
        ..Default::default()
    }))
}

pub(crate) fn vir_slot_single_object_report() -> Value {
    vir_slot_report(ExternalSource::Object(ExternalTray {
        tray_type: Some("PETG"),
        ..Default::default()
    }))
}

pub(crate) fn external_spool_single_array_report() -> Value {
    external_source_report(ExternalSource::Array(vec![ExternalTray {
        tray_type: Some("PLA"),
        ..Default::default()
    }]))
}

pub(crate) fn external_spool_multi_array_report() -> Value {
    external_source_report(ExternalSource::Array(vec![
        ExternalTray {
            tray_type: Some("PLA"),
            ..Default::default()
        },
        ExternalTray {
            tray_type: Some("PETG"),
            ..Default::default()
        },
    ]))
}

pub(crate) fn top_level_external_spool_report() -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            vt_tray: Some(ExternalSource::Array(vec![ExternalTray {
                id: Some(Scalar::U32(254)),
                extruder_id: Some(1),
                tray_info_idx: Some("GFG00"),
                tray_type: Some("PETG"),
                tray_color: Some("00FF00FF"),
                tray_sub_brands: Some("PETG HF"),
                remain: Some(Scalar::Str("64")),
                ..Default::default()
            }])),
            ams: MaterialAms {
                ams: vec![unit_with_tray(Scalar::U32(0), Scalar::U32(0))],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(crate) fn vir_slot_precedence_report() -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                vt_tray: Some(ExternalSource::Array(vec![
                    ExternalTray {
                        tray_type: Some("PLA"),
                        ..Default::default()
                    },
                    ExternalTray {
                        tray_type: Some("PETG"),
                        ..Default::default()
                    },
                ])),
                vir_slot: Some(ExternalSource::Array(vec![ExternalTray {
                    id: Some(Scalar::U32(255)),
                    setting_id: Some("GFSL05_07"),
                    ..Default::default()
                }])),
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(crate) fn active_tray_report(tray_now: u32) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                tray_now: Some(Scalar::U32(tray_now)),
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(crate) fn ams_ht_unit_report() -> Value {
    value(single_unit_report(MaterialAmsUnit {
        id: Scalar::U32(128),
        tray: vec![MaterialTray {
            id: Some(Scalar::U32(0)),
            tray_type: Some("PLA"),
            ..Default::default()
        }],
        info: None,
        humidity: None,
        humidity_raw: None,
        temp: None,
    }))
}

pub(crate) fn credential_filter_report() -> Value {
    value(single_unit_report(MaterialAmsUnit {
        id: Scalar::U32(0),
        tray: vec![MaterialTray {
            id: Some(Scalar::U32(0)),
            tray_color: Some("not-a-color"),
            access_code: Some("secret-access"),
            password: Some("secret-password"),
            passwd: Some("secret-passwd"),
            token: Some("secret-token"),
            auth: Some("secret-auth"),
            ..Default::default()
        }],
        info: None,
        humidity: None,
        humidity_raw: None,
        temp: None,
    }))
}

fn external_source_report(vt_tray: ExternalSource<'_>) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                vt_tray: Some(vt_tray),
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

fn vir_slot_report(vir_slot: ExternalSource<'_>) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                vir_slot: Some(vir_slot),
                ..Default::default()
            },
            ..Default::default()
        },
    })
}
