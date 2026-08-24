#![cfg(any(unix, windows))]

#[path = "logout_revoke/support.rs"]
mod support;

use std::{
    fs, thread,
    time::{Duration, Instant},
};
use support::{assert_no_request, next_request, run_probe, wait_for_client_close, write_response};

#[test]
fn requested_logout_revokes_the_current_plugin_session_and_clears_local_state() {
    let output = run_probe("success", |listener, deadline, _| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(&mut stream, "204 No Content", "");
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"success"}"#);
}

#[test]
fn local_only_logout_clears_state_without_revoking_the_plugin_session() {
    let output = run_probe("local", |listener, _, _| {
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"local"}"#);
}

#[test]
fn requested_logout_without_a_token_is_a_local_idempotent_no_op() {
    let output = run_probe("empty", |listener, _, _| {
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"empty"}"#);
}

#[test]
fn passive_logout_does_not_clear_a_login_committed_after_a_logged_out_observation() {
    let output = run_probe("stale-observation", |listener, _, _| {
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"stale-observation"}"#);
}

#[test]
fn failed_revoke_still_clears_printer_and_account_before_reporting_http_error() {
    let output = run_probe("failure", |listener, deadline, _| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(
            &mut stream,
            "500 Internal Server Error",
            r#"{"error":"raw-logout-failure","token":"logout-secret-token"}"#,
        );
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"failure"}"#);
}

#[test]
fn failed_revoke_is_durable_and_a_repeated_logout_retries_it() {
    let output = run_probe("failure-retry", |listener, deadline, _| {
        let (mut first, first_request) = next_request(&listener, deadline);
        assert!(first_request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        write_response(
            &mut first,
            "500 Internal Server Error",
            r#"{"error":"temporary_failure"}"#,
        );

        let (mut retry, retry_request) = next_request(&listener, deadline);
        assert!(retry_request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            retry_request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(&mut retry, "204 No Content", "");
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"failure-retry"}"#);
}

#[test]
fn failed_revoke_is_retried_before_a_restarted_agent_bootstraps_no_auth() {
    let output = run_probe("failure-restart", |listener, deadline, _| {
        let (mut first, first_request) = next_request(&listener, deadline);
        assert!(first_request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        write_response(
            &mut first,
            "500 Internal Server Error",
            r#"{"error":"temporary_failure"}"#,
        );

        let (mut retry, retry_request) = next_request(&listener, deadline);
        assert!(retry_request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        write_response(&mut retry, "204 No Content", "");

        let (mut bootstrap, bootstrap_request) = next_request(&listener, deadline);
        assert!(bootstrap_request.starts_with("POST /api/v1/plugin/no-auth-session HTTP/1.1"));
        write_response(
            &mut bootstrap,
            "200 OK",
            r#"{"token":"restart-token","profile":{"user_id":"restart-user","user_name":"Restart User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
        );
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"failure-restart"}"#);
}

#[test]
fn disconnected_revoke_still_clears_state_and_preserves_a_redacted_cause_chain() {
    let output = run_probe("disconnect", |listener, deadline, _| {
        let (stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        drop(stream);
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"disconnect"}"#);
}

#[test]
fn unresponsive_revoke_clears_local_state_promptly_and_has_a_finite_bound() {
    let output = run_probe("timeout", |listener, deadline, directory| {
        let (stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        wait_for_client_close(stream, deadline);
        assert_no_request(&listener, Instant::now() + Duration::from_secs(1));
        fs::write(directory.join("timeout-no-immediate-retry"), "").unwrap();
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"timeout"}"#);
}

#[test]
fn stale_unresponsive_revoke_does_not_report_into_a_replacement_account() {
    let output = run_probe("timeout-relogin", |listener, deadline, _| {
        let (stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        wait_for_client_close(stream, deadline);
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"timeout-relogin"}"#);
}

#[test]
fn repeated_requested_logout_does_not_repeat_the_delete() {
    let output = run_probe("repeat", |listener, deadline, _| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        write_response(&mut stream, "204 No Content", "");
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"repeat"}"#);
}

#[test]
fn local_clear_failure_keeps_a_tombstone_without_contacting_the_hub() {
    let output = run_probe("local-failure", |listener, _, _| {
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"local-failure"}"#);
}

#[test]
fn requested_logout_fences_startup_while_pending_revocation_delete_is_blocked() {
    let output = run_probe("bootstrap-logout-race", |listener, deadline, config| {
        let (mut bootstrap_delete, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        std::fs::write(config.join("bootstrap-delete-entered"), b"entered\n").unwrap();

        let (mut logout_delete, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        write_response(&mut logout_delete, "204 No Content", "");
        write_response(&mut bootstrap_delete, "204 No Content", "");
        assert_no_request(&listener, Instant::now() + Duration::from_millis(500));
    });
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"bootstrap-logout-race"}"#
    );
}

#[test]
fn requested_logout_fences_a_blocked_tokenless_ticket_exchange() {
    let output = run_probe("ticket-logout-race", |listener, deadline, config| {
        let (mut exchange, request) = next_request(&listener, deadline);
        assert!(request.starts_with("POST /api/v1/plugin/login-tickets/exchange HTTP/1.1"));
        std::fs::write(config.join("ticket-exchange-entered"), b"entered\n").unwrap();
        while !config.join("ticket-logout-complete").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(config.join("ticket-logout-complete").exists());
        write_response(
            &mut exchange,
            "200 OK",
            r#"{"token":"late-ticket-token","profile":{"user_id":"late-ticket-user","user_name":"Late Ticket User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
        );

        let (mut revoke, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer late-ticket-token")
        );
        write_response(&mut revoke, "204 No Content", "");
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"ticket-logout-race"}"#);
}

#[test]
fn passive_logout_does_not_fence_a_blocked_tokenless_ticket_exchange() {
    let output = run_probe("ticket-passive-control", |listener, deadline, config| {
        let (mut exchange, request) = next_request(&listener, deadline);
        assert!(request.starts_with("POST /api/v1/plugin/login-tickets/exchange HTTP/1.1"));
        std::fs::write(config.join("ticket-exchange-entered"), b"entered\n").unwrap();
        while !config.join("ticket-logout-complete").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(config.join("ticket-logout-complete").exists());
        write_response(
            &mut exchange,
            "200 OK",
            r#"{"token":"passive-ticket-token","profile":{"user_id":"passive-ticket-user","user_name":"Passive Ticket User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
        );
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"ticket-passive-control"}"#
    );
}

#[test]
fn staging_failure_falls_back_to_a_direct_successful_delete() {
    let output = run_probe("stage-failure-delete-success", |listener, deadline, _| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(&mut stream, "204 No Content", "");
    });
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"stage-failure-delete-success"}"#
    );
}

#[test]
fn staging_failure_keeps_the_login_until_direct_delete_succeeds() {
    let output = run_probe(
        "stage-failure-delete-delayed-success",
        |listener, deadline, config| {
            let (mut stream, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer logout-secret-token")
            );
            std::fs::write(config.join("unstaged-delete-entered"), b"entered\n").unwrap();
            while !config.join("release-unstaged-delete").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(config.join("release-unstaged-delete").exists());
            write_response(&mut stream, "204 No Content", "");
        },
    );
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"stage-failure-delete-delayed-success"}"#
    );
}

#[test]
fn unstaged_delete_never_clears_or_reports_into_a_replacement_login() {
    for (mode, status, body) in [
        ("stage-failure-delete-relogin-success", "204 No Content", ""),
        (
            "stage-failure-delete-relogin-failure",
            "503 Service Unavailable",
            r#"{"error":"raw-stage-delete-failure"}"#,
        ),
    ] {
        let output = run_probe(mode, move |listener, deadline, config| {
            let (mut stream, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer logout-secret-token")
            );
            std::fs::write(config.join("unstaged-delete-entered"), b"entered\n").unwrap();
            while !config.join("release-unstaged-delete").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(config.join("release-unstaged-delete").exists());
            write_response(&mut stream, status, body);
            assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
        });
        assert_eq!(output.trim(), format!(r#"{{"ok":true,"mode":"{mode}"}}"#));
    }
}

#[test]
fn staging_and_ambiguous_direct_delete_stay_logged_out_until_replay() {
    let output = run_probe("stage-failure-delete-failure", |listener, deadline, _| {
        let (mut first, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(
            &mut first,
            "503 Service Unavailable",
            r#"{"error":"raw-stage-delete-failure"}"#,
        );

        let (mut retry, request) = next_request(&listener, deadline);
        assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer logout-secret-token")
        );
        write_response(&mut retry, "204 No Content", "");
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(
        output.trim(),
        r#"{"ok":true,"mode":"stage-failure-delete-failure"}"#
    );
}

#[test]
fn staging_failure_treats_an_absent_remote_session_as_revoked() {
    for (mode, status) in [
        ("stage-failure-delete-unauthorized", "401 Unauthorized"),
        ("stage-failure-delete-gone", "410 Gone"),
    ] {
        let output = run_probe(mode, move |listener, deadline, _| {
            let (mut stream, request) = next_request(&listener, deadline);
            assert!(request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1"));
            write_response(&mut stream, status, r#"{"error":"already_absent"}"#);
        });
        assert_eq!(output.trim(), format!(r#"{{"ok":true,"mode":"{mode}"}}"#));
    }
}
