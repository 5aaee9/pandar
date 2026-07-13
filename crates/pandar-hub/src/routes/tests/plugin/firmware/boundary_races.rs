use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::super::*;
use super::support::*;
use crate::sessions::SessionToken;

#[tokio::test]
async fn firmware_execute_authoritative_claim_in_ownership_gap_is_side_effect_free() {
    let mut fixture = FirmwareRouteFixture::new("firmware-execute-ownership-gap").await;
    let (prepared, prepared_outbound) = fixture
        .prepare(start_metadata("ownership-gap", "ota", "v"))
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
        "command": start_command("ownership-gap", URL_SENTINEL, "ota", "v")
    });
    let mut pause =
        crate::firmware_control::execute_ownership_gap_pause::install(prepared_command_id);
    let request = fixture.spawn_json(Method::POST, "/execute", body.clone());
    pause.wait_until_reached().await;

    let sibling = sibling_state(&fixture.state);
    let replacement = SessionToken::new();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let replacement_claim = tokio::spawn({
        let sibling = sibling.clone();
        async move {
            sibling
                .agents()
                .claim_online_session(
                    tenant_id,
                    agent_id,
                    &replacement.persisted_id(),
                    "ownership-gap-replacement",
                    "2026-07-12T00:00:04Z",
                )
                .await
                .unwrap();
        }
    });
    tokio::time::timeout(Duration::from_secs(1), replacement_claim)
        .await
        .expect("replacement must claim ownership in the exposed gap")
        .unwrap();
    pause.resume();

    let (status, response) = request.await.unwrap();
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
    assert!(
        fixture
            .state
            .sessions()
            .firmware_token_locator(prepared["prepared_token"].as_str().unwrap())
            .is_some()
    );

    sibling
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            "ownership-gap-restored",
            "2026-07-12T00:00:05Z",
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
