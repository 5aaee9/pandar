use super::*;

#[test]
fn task_list_rotates_a_rejected_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("tasks", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20",
            Some("stale-token"),
        );
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "fresh-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20",
            Some("fresh-token"),
        );
        write_response(&mut stream, "200 OK", r#"{"total":0,"hits":[]}"#);
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"tasks"}"#);
}

#[test]
fn task_plate_rotates_a_gone_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("plate", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/plate",
            Some("stale-token"),
        );
        write_response(&mut stream, "410 Gone", r#"{"error":"expired_auth_token"}"#);
        rotate_session(&listener, deadline, "plate-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/plate",
            Some("plate-token"),
        );
        write_response(
            &mut stream,
            "200 OK",
            r#"{"studio_submission_id":42,"plate_index":3}"#,
        );
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"plate"}"#);
}

#[test]
fn subtask_rotates_a_rejected_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("subtask", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/subtask",
            Some("stale-token"),
        );
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "subtask-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/subtask",
            Some("subtask-token"),
        );
        write_response(
            &mut stream,
            "200 OK",
            r##"{"content":"{\"info\":{\"plate_idx\":3}}","context":{"plates":[{"index":3,"thumbnail":{"url":""},"prediction":120,"weight":12.5,"filaments":[{"color":"#FFFFFFFF","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##,
        );
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"subtask"}"#);
}

#[test]
fn task_read_does_not_rotate_or_retry_again_after_the_single_retry_is_rejected() {
    let output = run_probe("retry-rejected", |listener, deadline| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("stale-token"));
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "single-retry-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("single-retry-token"));
        write_response(&mut stream, "410 Gone", r#"{"error":"expired_auth_token"}"#);
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"retry-rejected"}"#);
}

#[test]
fn task_read_reports_the_no_auth_rotation_failure_instead_of_the_stale_401() {
    let output = run_probe("rotation-failure", |listener, deadline| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("stale-token"));
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );

        let (mut rotation, request) = next_request(&listener, deadline);
        assert_request(&request, "POST", "/api/v1/plugin/no-auth-session", None);
        write_response(
            &mut rotation,
            "409 Conflict",
            r#"{"error":"ambiguous_no_auth_tenant"}"#,
        );
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"rotation-failure"}"#);
}
