use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;

use super::*;

#[tokio::test]
async fn job_create_writes_artifact_queues_command_and_returns_created_job() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();

    let (status, body) = request(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        Some(json!({
            "filename": "plate file.3mf",
            "content_type": "model/3mf",
            "artifact_base64": STANDARD.encode(b"abc"),
            "plate_id": 1,
            "use_ams": true,
            "flow_cali": false,
            "timelapse": true
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "queued");
    assert_eq!(body["printer_id"], printer_id);
    assert_eq!(body["command"]["kind"], "print_project_file");
    assert_eq!(body["command"]["status"], "queued");
    assert_eq!(body["artifact"]["filename"], "plate_file.3mf");
    assert_eq!(body["artifact"]["size_bytes"], 3);
    assert_eq!(state.commands().count().await.unwrap(), 1);
}

#[tokio::test]
async fn job_create_rejects_invalid_tenant_printer_and_job_ids() {
    let app = app().await;
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/tenants/not-a-uuid/printers/printer/jobs",
        Some(valid_request()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));

    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/tenants/00000000-0000-0000-0000-000000000001/printers/not-a-uuid/jobs",
        Some(valid_request()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_printer_id" }));

    let (status, body) = request(
        app,
        Method::GET,
        "/api/v1/tenants/00000000-0000-0000-0000-000000000001/jobs/not-a-uuid",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_job_id" }));
}

#[tokio::test]
async fn job_create_rejects_missing_printer() {
    let app = app().await;
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/00000000-0000-0000-0000-000000000001/jobs"),
        Some(valid_request()),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "printer_not_found" }));
}

#[tokio::test]
async fn job_create_rejects_empty_invalid_and_oversized_artifacts() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &uri,
        Some(json!({ "filename": "plate.3mf", "content_type": "model/3mf", "artifact_base64": "", "plate_id": 1, "use_ams": false, "flow_cali": false, "timelapse": false })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &uri,
        Some(json!({ "filename": "plate.3mf", "content_type": "model/3mf", "artifact_base64": "@@@", "plate_id": 1, "use_ams": false, "flow_cali": false, "timelapse": false })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_artifact_base64" }));

    let oversized = vec![0_u8; state.job_storage().max_artifact_bytes() + 1];
    let (status, body) = request(
        app,
        Method::POST,
        &uri,
        Some(json!({ "filename": "plate.3mf", "content_type": "model/3mf", "artifact_base64": STANDARD.encode(oversized), "plate_id": 1, "use_ams": false, "flow_cali": false, "timelapse": false })),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body, json!({ "error": "artifact_too_large" }));
}

#[tokio::test]
async fn job_create_defaults_content_type_and_rejects_zero_plate() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &uri,
        Some(json!({
            "filename": "plate.3mf",
            "content_type": "",
            "artifact_base64": STANDARD.encode(b"abc"),
            "plate_id": 1,
            "use_ams": false,
            "flow_cali": false,
            "timelapse": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["artifact"]["content_type"], "application/octet-stream");

    let (status, body) = request(
        app,
        Method::POST,
        &uri,
        Some(json!({
            "filename": "plate.3mf",
            "content_type": "model/3mf",
            "artifact_base64": STANDARD.encode(b"abc"),
            "plate_id": 0,
            "use_ams": false,
            "flow_cali": false,
            "timelapse": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_plate_id" }));
}

#[tokio::test]
async fn job_list_and_detail_return_tenant_jobs() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = request(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        Some(valid_request()),
    )
    .await;
    let job_id = created["id"].as_str().unwrap();

    let (status, list) = request(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["jobs"].as_array().unwrap().len(), 1);

    let (status, detail) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{job_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], created["id"]);
    assert_eq!(detail["command"], created["command"]);
}

#[tokio::test]
async fn missing_job_detail_returns_not_found() {
    let app = app().await;
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/00000000-0000-0000-0000-000000000001"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "job_not_found" }));
}

fn valid_request() -> serde_json::Value {
    json!({
        "filename": "plate.3mf",
        "content_type": "model/3mf",
        "artifact_base64": STANDARD.encode(b"abc"),
        "plate_id": 1,
        "use_ams": false,
        "flow_cali": false,
        "timelapse": false
    })
}
