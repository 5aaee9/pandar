use super::*;

#[tokio::test]
async fn grpc_partial_snapshot_preserves_absent_telemetry_and_updates_present_field() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let mut full = snapshot("SN-PARTIAL", "Printer", "X2D", "PRINTING");
    full.nozzle_temperatures = vec![crate::protocol::agent::v1::NozzleTemperature {
        label: "L".to_owned(),
        current_celsius: "41".to_owned(),
        target_celsius: "220".to_owned(),
        diameter_mm: "0.4".to_owned(),
        nozzle_type: "Hardened steel".to_owned(),
        snow: None,
        hnow: None,
    }];
    full.active_nozzle = "L".to_owned();
    full.bed_temperature_celsius = "60".to_owned();
    full.bed_target_temperature_celsius = "65".to_owned();
    full.chamber_temperature_celsius = "32".to_owned();
    full.chamber_target_temperature_celsius = "45".to_owned();
    full.chamber_light_on = Some(true);
    handle_snapshot(&state, tenant_id, agent_id, token, full)
        .await
        .unwrap();

    let mut chamber_target_only = snapshot("SN-PARTIAL", "Printer", "X2D", " ");
    chamber_target_only.chamber_target_temperature_celsius = "48".to_owned();
    handle_snapshot(&state, tenant_id, agent_id, token, chamber_target_only)
        .await
        .unwrap();

    let stored = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(stored.status, "PRINTING");
    assert_eq!(stored.nozzle_temperatures.len(), 1);
    assert_eq!(stored.active_nozzle.as_deref(), Some("L"));
    assert_eq!(stored.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(stored.bed_target_temperature_celsius.as_deref(), Some("65"));
    assert_eq!(stored.chamber_temperature_celsius.as_deref(), Some("32"));
    assert_eq!(
        stored.chamber_target_temperature_celsius.as_deref(),
        Some("48")
    );
    assert_eq!(stored.chamber_light_on, Some(true));
}

#[tokio::test]
async fn grpc_authoritative_telemetry_snapshot_can_clear_stale_fields_independently() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let mut full = snapshot("SN-CLEAR", "Printer", "X2D", "PRINTING");
    full.nozzle_temperatures = vec![crate::protocol::agent::v1::NozzleTemperature {
        label: "L".to_owned(),
        current_celsius: "41".to_owned(),
        target_celsius: "220".to_owned(),
        diameter_mm: "0.4".to_owned(),
        nozzle_type: "Hardened steel".to_owned(),
        snow: None,
        hnow: None,
    }];
    full.active_nozzle = "L".to_owned();
    full.bed_temperature_celsius = "60".to_owned();
    full.bed_target_temperature_celsius = "65".to_owned();
    full.chamber_temperature_celsius = "32".to_owned();
    full.chamber_target_temperature_celsius = "45".to_owned();
    full.chamber_light_on = Some(true);
    handle_snapshot(&state, tenant_id, agent_id, token, full)
        .await
        .unwrap();

    let mut authoritative = snapshot("SN-CLEAR", "Printer", "X2D", "IDLE");
    authoritative.telemetry_authoritative = true;
    assert!(!authoritative.connection_authoritative);
    handle_snapshot(&state, tenant_id, agent_id, token, authoritative)
        .await
        .unwrap();

    let stored = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(stored.status, "IDLE");
    assert!(stored.nozzle_temperatures.is_empty());
    assert_eq!(stored.active_nozzle, None);
    assert_eq!(stored.bed_temperature_celsius, None);
    assert_eq!(stored.bed_target_temperature_celsius, None);
    assert_eq!(stored.chamber_temperature_celsius, None);
    assert_eq!(stored.chamber_target_temperature_celsius, None);
    assert_eq!(stored.chamber_light_on, None);
}
