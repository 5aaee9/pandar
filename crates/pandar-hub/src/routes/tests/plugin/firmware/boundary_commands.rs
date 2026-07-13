use axum::http::{Method, StatusCode};
use serde_json::json;

use super::super::*;
use super::support::*;
use crate::sessions::SessionToken;

#[tokio::test]
async fn plugin_firmware_denied_command_routes_never_persist_or_contact_agent() {
    let mut fixture = FirmwareRouteFixture::new("plugin-firmware-command-auth").await;
    let other = fixture
        .state
        .tenants()
        .create("plugin-firmware-command-other", "Other")
        .await
        .unwrap();
    let other_auth = plugin_studio_tenant_token(
        &fixture.state,
        &other.id.to_string(),
        "firmware-command-other",
    )
    .await;
    for (suffix, body) in [
        ("/refresh", json!({"sequence_id":"denied"})),
        ("/prepare", upgrade_metadata("denied")),
        (
            "/execute",
            json!({"prepared_token":"denied","command":upgrade_command("denied")}),
        ),
    ] {
        let (status, response) = request(
            fixture.app(),
            Method::POST,
            &fixture.uri(suffix),
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        if suffix == "/execute" {
            assert_eq!(response["phase"], "pre_publish_failure");
        }
        let (status, response) = request_as(
            fixture.app(),
            Method::POST,
            &fixture.uri(suffix),
            Some(body),
            &other_auth,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        if suffix == "/execute" {
            assert_eq!(response["phase"], "pre_publish_failure");
        }
    }
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    assert!(fixture.commands.try_recv().is_err());
}

#[tokio::test]
async fn plugin_firmware_unavailable_command_routes_do_not_persist_or_dispatch() {
    for (slug, capable, sibling) in [
        ("plugin-firmware-command-incapable", false, false),
        ("plugin-firmware-command-nonowner", true, true),
    ] {
        let mut fixture = FirmwareRouteFixture::with_capability(slug, capable).await;
        let app = if sibling {
            router(sibling_state(&fixture.state))
        } else {
            fixture.app()
        };
        let audit_count = fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .len();
        for (suffix, body) in [
            ("/refresh", json!({"sequence_id":"unavailable"})),
            ("/prepare", upgrade_metadata("unavailable")),
        ] {
            let (status, response) = request_as(
                app.clone(),
                Method::POST,
                &fixture.uri(suffix),
                Some(body),
                &fixture.auth,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(response["error"], "firmware_control_unavailable");
        }
        assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
        assert_eq!(
            fixture
                .state
                .audit_events()
                .list_for_tenant(fixture.tenant_id)
                .await
                .unwrap()
                .len(),
            audit_count
        );
        assert!(fixture.commands.try_recv().is_err());
    }

    let mut wrong_owner = FirmwareRouteFixture::new("plugin-firmware-command-wrong-owner").await;
    let other_agent = wrong_owner
        .state
        .agents()
        .create(wrong_owner.tenant_id, "command-other-owner")
        .await
        .unwrap();
    sqlx::query("UPDATE printers SET agent_id = ?1 WHERE id = ?2")
        .bind(other_agent.id.to_string())
        .bind(&wrong_owner.printer_id)
        .execute(sqlite_pool(&wrong_owner.state))
        .await
        .unwrap();
    for (suffix, body) in [
        ("/refresh", json!({"sequence_id":"wrong-owner"})),
        ("/prepare", upgrade_metadata("wrong-owner")),
    ] {
        let (status, response) = request_as(
            wrong_owner.app(),
            Method::POST,
            &wrong_owner.uri(suffix),
            Some(body),
            &wrong_owner.auth,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(response["error"], "firmware_control_unavailable");
    }
    assert_eq!(wrong_owner.state.commands().count().await.unwrap(), 0);
    assert!(wrong_owner.commands.try_recv().is_err());

    let mut execute = FirmwareRouteFixture::new("plugin-firmware-command-execute-nonowner").await;
    let (prepared, _) = execute
        .prepare(start_metadata("nonowner-execute", "ota", "v"))
        .await;
    let command_count = execute.state.commands().count().await.unwrap();
    let audit_count = execute
        .state
        .audit_events()
        .list_for_tenant(execute.tenant_id)
        .await
        .unwrap()
        .len();
    let body = json!({
        "prepared_token": prepared["prepared_token"],
        "command": start_command("nonowner-execute", URL_SENTINEL, "ota", "v")
    });
    let (status, response) = request_as(
        router(sibling_state(&execute.state)),
        Method::POST,
        &execute.uri("/execute"),
        Some(body.clone()),
        &execute.auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"], "firmware_control_unavailable");
    assert_eq!(response["phase"], "pre_publish_failure");
    assert!(!response.to_string().contains(URL_SENTINEL));
    assert!(!response.to_string().contains("user:secret"));
    assert_eq!(
        execute.state.commands().count().await.unwrap(),
        command_count
    );
    assert_eq!(
        execute
            .state
            .audit_events()
            .list_for_tenant(execute.tenant_id)
            .await
            .unwrap()
            .len(),
        audit_count
    );
    assert!(execute.commands.try_recv().is_err());

    let request = execute.spawn_json(Method::POST, "/execute", body);
    let outbound = execute.next_command().await;
    execute
        .generic_failure(command_id(&outbound), "known before publish")
        .await;
    let (status, response) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["phase"], "pre_publish_failure");
}

#[tokio::test]
async fn firmware_execute_former_owner_is_unavailable_without_consuming_token() {
    let mut fixture = FirmwareRouteFixture::new("firmware-execute-former-owner").await;
    let (prepared, prepared_outbound) = fixture
        .prepare(start_metadata("former-owner", "ota", "v"))
        .await;
    let prepared_command_id = command_id(&prepared_outbound);
    let command_before = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, prepared_command_id)
        .await
        .unwrap()
        .unwrap();
    let command_count = fixture.state.commands().count().await.unwrap();
    let audit_count = fixture
        .state
        .audit_events()
        .list_for_tenant(fixture.tenant_id)
        .await
        .unwrap()
        .len();
    let body = json!({
        "prepared_token": prepared["prepared_token"],
        "command": start_command("former-owner", URL_SENTINEL, "ota", "v")
    });

    let sibling = sibling_state(&fixture.state);
    let replacement = SessionToken::new();
    sibling
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &replacement.persisted_id(),
            "former-owner-replacement",
            "2026-07-12T00:00:02Z",
        )
        .await
        .unwrap();

    let (status, response) = request_as(
        fixture.app(),
        Method::POST,
        &fixture.uri("/execute"),
        Some(body.clone()),
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"], "firmware_control_unavailable");
    assert_eq!(response["phase"], "pre_publish_failure");
    assert!(!response.to_string().contains(URL_SENTINEL));
    assert!(!response.to_string().contains("user:secret"));
    assert_eq!(
        fixture.state.commands().count().await.unwrap(),
        command_count
    );
    assert_eq!(
        fixture
            .state
            .commands()
            .get_for_tenant(fixture.tenant_id, prepared_command_id)
            .await
            .unwrap()
            .unwrap(),
        command_before
    );
    assert_eq!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .len(),
        audit_count
    );
    assert!(fixture.commands.try_recv().is_err());

    sibling
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            "former-owner-restored",
            "2026-07-12T00:00:03Z",
        )
        .await
        .unwrap();
    let request = fixture.spawn_json(Method::POST, "/execute", body);
    let outbound = fixture.next_command().await;
    fixture
        .generic_failure(command_id(&outbound), "known before publish")
        .await;
    let (status, response) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["phase"], "pre_publish_failure");
}
