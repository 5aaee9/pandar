#![cfg(any(unix, windows))]

#[allow(dead_code)]
#[path = "logout_revoke/support.rs"]
mod support;

use std::time::{Duration, Instant};

use support::{assert_no_request, next_request, run_fixture_probe, write_response};

#[test]
fn synchronous_false_login_callback_reentrant_requested_logout_deletes_once() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "reentrant-success",
        |listener, deadline, _| {
            let (mut stream, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer reentrant-upgrade-token")
            );
            write_response(&mut stream, "204 No Content", "");
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"reentrant-success"}"#);
}

#[test]
fn upgraded_delete_failure_is_reported_once_without_repeating_logout_callback() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "reentrant-failure",
        |listener, deadline, _| {
            let (mut stream, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            write_response(
                &mut stream,
                "503 Service Unavailable",
                r#"{"error":"raw-upgrade-delete-failure","token":"reentrant-upgrade-token"}"#,
            );
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"reentrant-failure"}"#);
}

#[test]
fn direct_intent_persistence_failure_restores_profile_before_any_delete() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "reentrant-retained-failure",
        |listener, deadline, config| {
            let restored = config.join("retained-restore-complete");
            while !restored.exists() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(restored.exists());
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
            std::fs::write(config.join("release-retained-retry"), b"release\n").unwrap();

            let (mut retry, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer reentrant-upgrade-token")
            );
            write_response(&mut retry, "204 No Content", "");
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"reentrant-retained-failure"}"#
    );
}

#[test]
fn uncertain_direct_delete_stays_logged_out_until_the_intent_replays() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "reentrant-retained-disconnect",
        |listener, deadline, _| {
            let (disconnected, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer reentrant-upgrade-token")
            );
            drop(disconnected);

            let (mut retry, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer reentrant-upgrade-token")
            );
            write_response(&mut retry, "204 No Content", "");
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"reentrant-retained-disconnect"}"#
    );
}

#[test]
fn passive_cleanup_failure_restores_without_http_or_delete() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "passive-restore",
        |listener, _, _| {
            assert_no_request(&listener, Instant::now() + Duration::from_millis(500));
        },
    );
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"passive-restore"}"#);
}

#[test]
fn requested_after_passive_empty_fences_an_already_sent_no_auth_post() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "late-no-auth-post",
        |listener, deadline, config| {
            let (mut session, request) = next_request(&listener, deadline);
            assert!(request.starts_with("POST /api/v1/plugin/no-auth-session HTTP/1.1"));
            std::fs::write(config.join("no-auth-post-entered"), b"entered\n").unwrap();
            while !config.join("logout-complete").exists() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(config.join("logout-complete").exists());
            write_response(
                &mut session,
                "200 OK",
                r#"{"token":"late-no-auth-secret-token","profile":{"user_id":"late-user","user_name":"Late User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
            );
            let (mut revoke, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer late-no-auth-secret-token")
            );
            write_response(&mut revoke, "204 No Content", "");
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"late-no-auth-post"}"#);
}

#[test]
fn requested_after_passive_empty_fences_an_already_sent_ticket_exchange() {
    let output = run_fixture_probe(
        "logout_revoke_upgrade_probe.cpp",
        "late-ticket-passive-requested",
        |listener, deadline, config| {
            let (mut exchange, request) = next_request(&listener, deadline);
            assert!(request.starts_with("POST /api/v1/plugin/login-tickets/exchange HTTP/1.1"));
            std::fs::write(config.join("ticket-post-entered"), b"entered\n").unwrap();
            while !config.join("logout-complete").exists() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(config.join("logout-complete").exists());
            write_response(
                &mut exchange,
                "200 OK",
                r#"{"token":"late-ticket-after-passive-token","profile":{"user_id":"late-ticket-user","user_name":"Late Ticket User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
            );
            let (mut revoke, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer late-ticket-after-passive-token")
            );
            write_response(&mut revoke, "204 No Content", "");
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        },
    );
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"late-ticket-passive-requested"}"#
    );
}
