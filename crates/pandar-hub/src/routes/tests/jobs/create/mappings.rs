use serde::Serialize;
use serde_json::Value;

pub(super) struct InvalidMaterialMappingCase {
    pub(super) ams_mapping: Option<Value>,
    pub(super) ams_mapping2: Option<Value>,
}

pub(super) fn empty_ams_mapping() -> Value {
    value(Vec::<i32>::new())
}

pub(super) fn empty_ams_mapping2() -> Value {
    value(Vec::<AmsMapping2Request>::new())
}

pub(super) fn external_ams_mapping2() -> Value {
    value([AmsMapping2Request {
        ams_id: 254,
        slot_id: 8,
    }])
}

pub(super) fn invalid_material_mapping_cases() -> Vec<InvalidMaterialMappingCase> {
    vec![
        InvalidMaterialMappingCase {
            ams_mapping: Some(value("sk-live-secret")),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(value(["sk-live-secret"])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(value([2147483648_i64])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(value(vec![0; 33])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value("sk-live-secret")),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value([AmsMapping2StringAmsId {
                ams_id: "sk-live-secret",
                slot_id: 0,
            }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value([AmsMapping2LargeSlotId {
                ams_id: 0,
                slot_id: 2147483648_i64,
            }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value([AmsMapping2Password {
                ams_id: 0,
                slot_id: 0,
                password: "sk-live-secret",
            }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value([AmsMapping2Token {
                ams_id: 0,
                slot_id: 0,
                token: "sk-live-secret",
            }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value([AmsMapping2AccessCode {
                ams_id: 0,
                slot_id: 0,
                access_code: "sk-live-secret",
            }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(value(vec![
                AmsMapping2Request {
                    ams_id: 0,
                    slot_id: 0
                };
                33
            ])),
        },
    ]
}

#[derive(Clone, Serialize)]
struct AmsMapping2Request {
    ams_id: i32,
    slot_id: i32,
}

#[derive(Serialize)]
struct AmsMapping2StringAmsId {
    ams_id: &'static str,
    slot_id: i32,
}

#[derive(Serialize)]
struct AmsMapping2LargeSlotId {
    ams_id: i32,
    slot_id: i64,
}

#[derive(Serialize)]
struct AmsMapping2Password {
    ams_id: i32,
    slot_id: i32,
    password: &'static str,
}

#[derive(Serialize)]
struct AmsMapping2Token {
    ams_id: i32,
    slot_id: i32,
    token: &'static str,
}

#[derive(Serialize)]
struct AmsMapping2AccessCode {
    ams_id: i32,
    slot_id: i32,
    access_code: &'static str,
}

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
