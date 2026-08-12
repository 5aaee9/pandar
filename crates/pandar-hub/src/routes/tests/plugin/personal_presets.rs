use std::collections::BTreeMap;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use super::*;

#[derive(Debug, Clone, Serialize)]
struct PresetBody {
    #[serde(rename = "type")]
    preset_type: &'static str,
    name: String,
    version: &'static str,
    base_id: String,
    inherits: Option<String>,
    filament_id: Option<String>,
    options: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct MutationResponse {
    setting_id: String,
    updated_time: i64,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    message: String,
    presets: Vec<PresetMetadata>,
}

#[derive(Debug, Deserialize)]
struct PresetMetadata {
    setting_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct PresetResponse {
    setting_id: String,
    #[serde(rename = "type")]
    preset_type: String,
    name: String,
    base_id: String,
    options: BTreeMap<String, String>,
}

fn preset(name: &str) -> PresetBody {
    PresetBody {
        preset_type: "print",
        name: name.to_owned(),
        version: "2.8.1.55",
        base_id: String::new(),
        inherits: Some("0.20mm Standard".to_owned()),
        filament_id: None,
        options: BTreeMap::from([("layer_height".to_owned(), "0.16".to_owned())]),
    }
}

#[tokio::test]
async fn personal_preset_routes_round_trip_replay_replace_list_and_delete() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("preset-routes", "Presets")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "preset-owner").await;
    let app = router(state.clone());

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/presets",
        Some(json_body(preset("Fine"))),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<MutationResponse>(created);

    let (status, replay) = request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/presets",
        Some(json_body(preset("Fine"))),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replay = decode::<MutationResponse>(replay);
    assert_eq!(replay.setting_id, created.setting_id);
    assert_eq!(replay.updated_time, created.updated_time);

    let response = raw_request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/presets?bundle_version=2.8.1.55",
        &token,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let listed: ListResponse =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(listed.message, "success");
    assert_eq!(listed.presets.len(), 1);
    assert_eq!(listed.presets[0].setting_id, created.setting_id);
    assert_eq!(listed.presets[0].name, "Fine");

    let (status, full) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/plugin/presets/{}", created.setting_id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let full = decode::<PresetResponse>(full);
    assert_eq!(full.setting_id, created.setting_id);
    assert_eq!(full.preset_type, "print");
    assert_eq!(full.name, "Fine");
    assert_eq!(full.base_id, "");
    assert_eq!(full.options["layer_height"], "0.16");
    assert!(!full.options.contains_key("setting_id"));
    assert!(!full.options.contains_key("user_id"));

    let mut replacement = preset("Fine Updated");
    replacement
        .options
        .insert("wall_loops".to_owned(), "3".to_owned());
    let (status, replaced) = request_as(
        app.clone(),
        Method::PATCH,
        &format!("/api/v1/plugin/presets/{}", created.setting_id),
        Some(json_body(replacement)),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode::<MutationResponse>(replaced).updated_time > created.updated_time);

    for _ in 0..2 {
        let response = raw_request_as(
            app.clone(),
            Method::DELETE,
            &format!("/api/v1/plugin/presets/{}", created.setting_id),
            &token,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "personal_preset.create")
            .count(),
        1
    );
    for event in events
        .iter()
        .filter(|event| event.target_type == "personal_preset")
    {
        assert!(!event.metadata_json.contains("layer_height"));
        assert!(!event.metadata_json.contains("wall_loops"));
    }
}

#[tokio::test]
async fn personal_preset_routes_validate_auth_version_duplicates_and_ownership() {
    let state = state().await;
    let first = state.tenants().create("preset-auth-a", "A").await.unwrap();
    let second = state.tenants().create("preset-auth-b", "B").await.unwrap();
    let first_token =
        plugin_studio_tenant_token(&state, &first.id.to_string(), "first-owner").await;
    let same_tenant_other =
        plugin_studio_tenant_token(&state, &first.id.to_string(), "same-tenant-other").await;
    let second_token =
        plugin_studio_tenant_token(&state, &second.id.to_string(), "second-owner").await;
    let wrong_scope = all_scope_tenant_token(&state, &first.id.to_string(), "wrong-scope").await;
    let app = router(state.clone());

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/presets",
        Some(json_body(preset("Owned"))),
        &first_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<MutationResponse>(created);

    for (token, expected, error) in [
        (
            &same_tenant_other,
            StatusCode::NOT_FOUND,
            "personal_preset_not_found",
        ),
        (
            &second_token,
            StatusCode::NOT_FOUND,
            "personal_preset_not_found",
        ),
        (&wrong_scope, StatusCode::FORBIDDEN, "role_forbidden"),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("/api/v1/plugin/presets/{}", created.setting_id),
            None,
            token,
        )
        .await;
        assert_eq!(status, expected);
        assert_eq!(decode::<ErrorResponse>(body).error, error);
    }

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/presets?bundle_version=bad",
        None,
        &first_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "invalid_personal_preset"
    );

    let authenticated = state
        .auth()
        .authenticate_tenant_token(&first_token)
        .await
        .unwrap()
        .unwrap();
    let owner_id = authenticated.session_user.unwrap().id;
    crate::entities::users::Entity::update_many()
        .filter(crate::entities::users::Column::Id.eq(&owner_id))
        .set(crate::entities::users::ActiveModel {
            role: Set("viewer".to_owned()),
            ..Default::default()
        })
        .exec(&state.database().sea_orm_connection())
        .await
        .unwrap();
    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/presets?bundle_version=2.8.1.55",
        None,
        &first_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    crate::entities::users::Entity::update_many()
        .filter(crate::entities::users::Column::Id.eq(owner_id))
        .set(crate::entities::users::ActiveModel {
            role: Set("operator".to_owned()),
            ..Default::default()
        })
        .exec(&state.database().sea_orm_connection())
        .await
        .unwrap();
    let mut conflict = preset("Owned");
    conflict.preset_type = "filament";
    conflict.filament_id = Some("P123".to_owned());
    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/plugin/presets",
        Some(json_body(conflict)),
        &first_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "personal_preset_name_conflict"
    );
}

#[tokio::test]
async fn personal_preset_routes_reject_ownerless_no_auth_and_return_quota_code_14() {
    let state = raw_state().await.with_no_auth_for_tests(true);
    let tenant = state
        .tenants()
        .create("preset-no-auth", "No Auth")
        .await
        .unwrap();
    let (status, session) = request(
        router(state.clone()),
        Method::POST,
        "/api/v1/plugin/no-auth-session",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session = decode::<ExchangeLoginTicketResponse>(session);
    let app = router(state.clone());
    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/presets?bundle_version=2.8.1.55",
        None,
        &session.token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "personal_presets_require_user"
    );

    let authenticated = state
        .auth()
        .authenticate_tenant_token(&session.token)
        .await
        .unwrap()
        .unwrap();
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "quota@test",
            "Quota",
            crate::repositories::UserRole::Operator,
        )
        .await
        .unwrap();
    let stored = crate::entities::tenant_tokens::Entity::find_by_id(&authenticated.token.id)
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active: crate::entities::tenant_tokens::ActiveModel = stored.into();
    active.created_by_user_id = Set(Some(user.id.clone()));
    active
        .update(&state.database().sea_orm_connection())
        .await
        .unwrap();
    let now = pandar_core::created_at_now();
    crate::entities::personal_presets::Entity::insert_many((0..1_000).map(|index| {
        crate::entities::personal_presets::ActiveModel {
            id: Set(uuid::Uuid::new_v4().to_string()),
            tenant_id: Set(tenant.id.to_string()),
            owner_user_id: Set(user.id.clone()),
            preset_type: Set("print".to_owned()),
            name: Set(format!("P {index}")),
            version: Set("2.8.1.55".to_owned()),
            base_id: Set(String::new()),
            inherits: Set(None),
            filament_id: Set(None),
            options_json: Set("{}".to_owned()),
            updated_time: Set(i64::from(index)),
            created_at: Set(now.clone()),
            updated_at: Set(now.clone()),
        }
    }))
    .exec_without_returning(&state.database().sea_orm_connection())
    .await
    .unwrap();
    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/plugin/presets",
        Some(json_body(preset("Overflow"))),
        &session.token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "personal_preset_limit_exceeded");
    assert_eq!(body["code"], 14);
}

#[tokio::test]
async fn personal_preset_route_rejects_malformed_and_oversize_bodies() {
    let state = state().await;
    let tenant = state.tenants().create("preset-body", "Body").await.unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "body-owner").await;
    let app = router(state);
    let malformed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plugin/presets")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let oversize = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plugin/presets")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from("x".repeat(513 * 1024)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversize.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
