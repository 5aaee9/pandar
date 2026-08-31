use super::*;

#[tokio::test]
async fn plugin_printer_list_returns_studio_devices_shape() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-device-list", "Plugin Device List")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "devices").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let session = register_feature_session(&state, tenant.id, agent.id, false).await;
    let printer = state
        .printers()
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            &session.persisted_id(),
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "studio-printer-1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("studio-access-code".to_string()),
                name: "Studio Printer".to_string(),
                model: Some("Bambu Lab X2D".to_string()),
                status: Some("IDLE".to_string()),
                observed_at: "2026-06-20T00:00:00Z".to_string(),
                nozzle_temperatures: vec![
                    pandar_core::PrinterNozzleTemperature {
                        label: Some("L".to_string()),
                        current_celsius: Some("28".to_string()),
                        target_celsius: Some("220".to_string()),
                        diameter_mm: None,
                        nozzle_type: None,
                        snow: None,
                        hnow: None,
                    },
                    pandar_core::PrinterNozzleTemperature {
                        label: Some("R".to_string()),
                        current_celsius: Some("27".to_string()),
                        target_celsius: Some("215".to_string()),
                        diameter_mm: None,
                        nozzle_type: None,
                        snow: None,
                        hnow: None,
                    },
                ],
                active_nozzle: Some("L".to_string()),
                bed_temperature_celsius: Some("60".to_string()),
                bed_target_temperature_celsius: Some("65".to_string()),
                chamber_temperature_celsius: Some("32".to_string()),
                chamber_target_temperature_celsius: None,
                chamber_light_on: Some(true),
                cooling_system: None,
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
            None,
        )
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id: printer.id.clone(),
            serial_number: "studio-printer-1".to_string(),
            printer_materials_json: serde_json::to_string(&PluginMaterialPatchFixture {
                kind: "printer_material_patch",
                observed_at: "2026-06-20T00:01:00Z",
                cfg: "8000000000000001",
                aux: "A4003001",
                stat: "1000000001",
                ams_units: [PluginMaterialPatchAmsUnit {
                    unit_id: "0",
                    info: "00000E00",
                    humidity: 25,
                    humidity_level: 3,
                    temperature_celsius: 28.5,
                    toolhead: "R",
                    trays: [PluginMaterialPatchTray {
                        tray_id: "0",
                        global_tray_id: 0,
                        material_type: "PLA",
                        filament_id: "GFL99",
                        color: "00FF00",
                        remaining_estimate: "72",
                    }],
                }],
                external_spools: [PluginMaterialPatchExternalSpool {
                    external_id: "254",
                    tray_id: "0",
                    material_type: "PETG",
                    filament_id: "GFG00",
                    color: "11223344",
                    toolhead: "L",
                }],
                active_tray: PluginMaterialPatchActiveTray {
                    kind: "ams",
                    ams_id: "0",
                    tray_id: "0",
                    global_tray_id: 0,
                },
            })
            .unwrap(),
        })
        .await
        .unwrap();

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let device_json = &body["devices"][0];
    assert!(device_json.get("dev_ip").is_none());
    assert!(device_json.get("dev_access_code").is_none());
    let encoded = body.to_string();
    assert!(!encoded.contains("192.0.2.10"));
    assert!(!encoded.contains("studio-access-code"));
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert_eq!(body.printers, None);
    assert_eq!(body.devices.len(), 1);
    let device = &body.devices[0];
    assert_eq!(device.dev_id, "studio-printer-1");
    assert_eq!(device.fun, "0");
    assert_eq!(device.dev_name, "Studio Printer");
    assert_eq!(device.name, "Studio Printer");
    assert_eq!(device.dev_model_name.as_deref(), Some("N6"));
    assert_eq!(device.model.as_deref(), Some("Bambu Lab X2D"));
    assert!(device.dev_online);
    assert!(device.online);
    assert_eq!(device.task_status, "IDLE");
    assert_eq!(device.state, "IDLE");
    assert_eq!(device.gcode_state, None);
    assert_eq!(device.pandar_printer_id, printer.id);
    assert_eq!(
        device.nozzle_temperatures[0].current_celsius.as_deref(),
        Some("28")
    );
    assert_eq!(
        device.nozzle_temperatures[0].target_celsius.as_deref(),
        Some("220")
    );
    assert_eq!(
        device.nozzle_temperatures[1].current_celsius.as_deref(),
        Some("27")
    );
    assert_eq!(
        device.nozzle_temperatures[1].target_celsius.as_deref(),
        Some("215")
    );
    assert_eq!(device.active_nozzle.as_deref(), Some("L"));
    assert_eq!(device.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(device.bed_target_temperature_celsius.as_deref(), Some("65"));
    assert_eq!(device.chamber_temperature_celsius.as_deref(), Some("32"));
    assert_eq!(device.chamber_light_on, Some(true));
    let materials = device.materials.as_ref().unwrap();
    assert_eq!(materials.cfg, "8000000000000001");
    assert_eq!(materials.aux, "A4003001");
    assert_eq!(materials.stat, "1000000001");
    assert_eq!(materials.ams_units[0].unit_id, "0");
    assert_eq!(materials.ams_units[0].info, "00000E00");
    assert_eq!(materials.external_spools[0].external_id, "254");
    assert_eq!(materials.active_tray.global_tray_id, 0);
}

#[tokio::test]
async fn h2c_rack_projection_uses_one_current_session_snapshot_for_all_capabilities() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-h2c-rack", "Plugin H2C Rack")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "h2c-rack").await;
    let agent_id = feature_advertisement_printer_with_model(
        &state,
        tenant.id,
        "h2c-agent",
        "H2C-RACK",
        "O1C2",
    )
    .await;
    let session = register_capability_session(
        &state,
        tenant.id,
        agent_id,
        [
            pandar_protocol::agent::v1::AgentCapability::RequiredDeviceFeatures,
            pandar_protocol::agent::v1::AgentCapability::H2cAutoNozzleMapping,
        ],
    )
    .await;
    let nozzle_system = serde_json::from_value(serde_json::json!({
        "nozzle": {
            "exist": 65536,
            "state": 0,
            "src_id": 16,
            "tar_id": 17,
            "info": [{"id": 16, "diameter": 0.4, "type": "XS01", "stat": 0}]
        },
        "holder": {"stat": 0, "pos": 2, "info": 0}
    }))
    .unwrap();
    state
        .printers()
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent_id,
            &session.persisted_id(),
            crate::repositories::PrinterSnapshotUpsert {
                serial_number: "H2C-RACK".to_owned(),
                host: None,
                access_code: None,
                name: "H2C Rack".to_owned(),
                model: Some("O1C2".to_owned()),
                status: Some("idle".to_owned()),
                observed_at: "2026-08-01T00:00:00Z".to_owned(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_target_temperature_celsius: None,
                chamber_light_on: None,
                cooling_system: None,
                nozzle_system: Some(nozzle_system),
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
            Some(pandar_core::BambuDeviceFeatures::from_bits(1_u64 << 60)),
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["fun"], "1000000000000000");
    assert!(body["devices"][0]["online"].as_bool().unwrap());
    assert!(body["devices"][0].get("fun2").is_none());
    assert_eq!(
        body["devices"][0]["nozzle_system"]["nozzle"]["info"][0]["id"],
        16
    );

    register_capability_session(
        &state,
        tenant.id,
        agent_id,
        [
            pandar_protocol::agent::v1::AgentCapability::RequiredDeviceFeatures,
            pandar_protocol::agent::v1::AgentCapability::H2cAutoNozzleMapping,
        ],
    )
    .await;
    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["devices"][0]["fun"], "0");
    assert!(!body["devices"][0]["online"].as_bool().unwrap());
    assert!(body["devices"][0].get("nozzle_system").is_none());
}

#[tokio::test]
async fn plugin_printer_fun_requires_current_capable_observation_session() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-device-features", "Plugin Device Features")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "device-features").await;

    let matching_agent =
        feature_advertisement_printer(&state, tenant.id, "matching-agent", "FUN-MATCHING").await;
    let matching_token = register_feature_session(&state, tenant.id, matching_agent, true).await;
    set_device_features(
        &state,
        tenant.id,
        matching_agent,
        matching_token,
        "FUN-MATCHING",
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020,
        )),
    )
    .await;

    let incapable_agent =
        feature_advertisement_printer(&state, tenant.id, "incapable-agent", "FUN-INCAPABLE").await;
    let incapable_token = register_feature_session(&state, tenant.id, incapable_agent, false).await;
    set_device_features(
        &state,
        tenant.id,
        incapable_agent,
        incapable_token,
        "FUN-INCAPABLE",
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020,
        )),
    )
    .await;

    let replaced_agent =
        feature_advertisement_printer(&state, tenant.id, "replaced-agent", "FUN-REPLACED").await;
    let old_token = register_feature_session(&state, tenant.id, replaced_agent, true).await;
    set_device_features(
        &state,
        tenant.id,
        replaced_agent,
        old_token,
        "FUN-REPLACED",
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020,
        )),
    )
    .await;
    register_feature_session(&state, tenant.id, replaced_agent, true).await;

    let disconnected_agent =
        feature_advertisement_printer(&state, tenant.id, "disconnected-agent", "FUN-DISCONNECTED")
            .await;
    let disconnected_token = crate::sessions::SessionToken::new();
    claim_feature_session(&state, tenant.id, disconnected_agent, disconnected_token).await;
    set_device_features(
        &state,
        tenant.id,
        disconnected_agent,
        disconnected_token,
        "FUN-DISCONNECTED",
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020,
        )),
    )
    .await;

    let invalidated_agent =
        feature_advertisement_printer(&state, tenant.id, "invalidated-agent", "FUN-INVALIDATED")
            .await;
    let invalidated_token =
        register_feature_session(&state, tenant.id, invalidated_agent, true).await;
    set_device_features(
        &state,
        tenant.id,
        invalidated_agent,
        invalidated_token,
        "FUN-INVALIDATED",
        Some(pandar_core::BambuDeviceFeatures::from_bits(
            0x8000_0041_0000_0020,
        )),
    )
    .await;
    set_device_features(
        &state,
        tenant.id,
        invalidated_agent,
        invalidated_token,
        "FUN-INVALIDATED",
        None,
    )
    .await;

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    let fun = decode::<PluginPrinterListResponse>(body)
        .devices
        .into_iter()
        .map(|device| (device.dev_id, device.fun))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fun["FUN-MATCHING"], "8000004100000020");
    assert_eq!(fun["FUN-INCAPABLE"], "0");
    assert_eq!(fun["FUN-REPLACED"], "0");
    assert_eq!(fun["FUN-DISCONNECTED"], "0");
    assert_eq!(fun["FUN-INVALIDATED"], "0");
}

#[tokio::test]
async fn plugin_printer_fun2_requires_current_capable_observation_session() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("device-features-2", "Plugin Secondary Device Features")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "device-features-2").await;

    let secondary = pandar_core::BambuDeviceFeatures::from_bits(0x8000_0000_0000_00A3);

    let matching_agent =
        feature_advertisement_printer(&state, tenant.id, "fun2-matching", "FUN2-MATCHING").await;
    let matching_token = register_feature_session(&state, tenant.id, matching_agent, true).await;
    set_secondary_device_features(
        &state,
        tenant.id,
        matching_agent,
        matching_token,
        "FUN2-MATCHING",
        Some(secondary),
    )
    .await;

    let incapable_agent =
        feature_advertisement_printer(&state, tenant.id, "fun2-incapable", "FUN2-INCAPABLE").await;
    let incapable_token = register_feature_session(&state, tenant.id, incapable_agent, false).await;
    set_secondary_device_features(
        &state,
        tenant.id,
        incapable_agent,
        incapable_token,
        "FUN2-INCAPABLE",
        Some(secondary),
    )
    .await;

    let replaced_agent =
        feature_advertisement_printer(&state, tenant.id, "fun2-replaced", "FUN2-REPLACED").await;
    let old_token = register_feature_session(&state, tenant.id, replaced_agent, true).await;
    set_secondary_device_features(
        &state,
        tenant.id,
        replaced_agent,
        old_token,
        "FUN2-REPLACED",
        Some(secondary),
    )
    .await;
    register_feature_session(&state, tenant.id, replaced_agent, true).await;

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    let fun2 = decode::<PluginPrinterListResponse>(body)
        .devices
        .into_iter()
        .map(|device| (device.dev_id, device.fun2))
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(fun2["FUN2-MATCHING"].as_deref(), Some("80000000000000A3"));
    assert_eq!(fun2["FUN2-INCAPABLE"], None);
    assert_eq!(fun2["FUN2-REPLACED"], None);
}
