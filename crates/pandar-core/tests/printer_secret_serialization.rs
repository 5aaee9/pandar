use pandar_core::{AgentId, Printer, TenantId};

#[test]
fn printer_serialization_omits_access_code() {
    let printer = Printer {
        id: "printer-1".to_owned(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        serial_number: "SERIAL".to_owned(),
        host: Some("192.168.1.2".to_owned()),
        access_code: Some("printer-secret".to_owned()),
        name: "Printer".to_owned(),
        model: Some("X1C".to_owned()),
        status: "idle".to_owned(),
        last_seen_at: "2026-07-25T00:00:00Z".to_owned(),
        created_at: "2026-07-25T00:00:00Z".to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        bambu_device_features: None,
        bambu_device_features_session_id: None,
        mqtt_presence_session_id: None,
    };

    let serialized = serde_json::to_string(&printer).unwrap();
    assert!(!serialized.contains("access_code"));
    assert!(!serialized.contains("printer-secret"));
}
