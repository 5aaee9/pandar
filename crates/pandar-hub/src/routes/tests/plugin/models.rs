use super::*;

#[tokio::test]
async fn plugin_printer_list_projects_every_studio_model_resource_id() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-model-ids", "Plugin Model IDs")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "model-ids").await;
    let models = [
        ("a1-mini", "Bambu Lab A1 Mini", "N1"),
        ("a1", "Bambu Lab A1", "N2S"),
        ("x1", "Bambu Lab X1", "BL-P002"),
        ("x1c", "Bambu Lab X1 Carbon", "BL-P001"),
        ("x1e", "Bambu Lab X1E", "C13"),
        ("p1p", "Bambu Lab P1P", "C11"),
        ("p1s", "Bambu Lab P1S", "C12"),
        ("p2s", "Bambu Lab P2S", "N7"),
        ("x2d", "Bambu Lab X2D", "N6"),
        ("a2l", "Bambu Lab A2L", "N9"),
        ("h2s", "Bambu Lab H2S", "O1S"),
        ("h2d", "Bambu Lab H2D", "O1D"),
        ("h2d-pro", "Bambu Lab H2D Pro", "O1E"),
        ("h2c", "Bambu Lab H2C", "O1C2"),
        ("h2c-alias", "O1C", "O1C2"),
        ("unknown", "Custom Prototype", "Custom Prototype"),
    ];
    for (serial, model, _) in models {
        feature_advertisement_printer_with_model(
            &state,
            tenant.id,
            &format!("agent-{serial}"),
            serial,
            model,
        )
        .await;
    }

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let devices = decode::<PluginPrinterListResponse>(body)
        .devices
        .into_iter()
        .map(|device| (device.dev_id, device.dev_model_name.unwrap()))
        .collect::<std::collections::HashMap<_, _>>();
    for (serial, _, expected) in models {
        assert_eq!(devices.get(serial).map(String::as_str), Some(expected));
    }
}
