use super::*;

#[test]
fn concurrent_task_401_responses_share_one_no_auth_rotation() {
    run("concurrent", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut first, first_request) = request(&listener, deadline);
        let (mut second, second_request) = request(&listener, deadline);
        assert_request(&first_request, "GET", path, Some("stale-token"));
        assert_request(&second_request, "GET", path, Some("stale-token"));
        respond(
            &mut first,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        respond(
            &mut second,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        respond(&mut rotate, "200 OK", &candidate("shared-token"));
        for _ in 0..2 {
            let (mut retry, retry_request) = request(&listener, deadline);
            assert_request(&retry_request, "GET", path, Some("shared-token"));
            respond(&mut retry, "200 OK", r#"{"total":0,"hits":[]}"#);
        }
        no_more_requests(&listener, Duration::from_millis(250));
    });
}

#[test]
fn authenticated_task_401_does_not_fall_back_to_no_auth() {
    run("authenticated", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut tasks, tasks_request) = request(&listener, deadline);
        assert_request(&tasks_request, "GET", path, Some("stale-token"));
        respond(
            &mut tasks,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        no_more_requests(&listener, Duration::from_millis(500));
    });
}

#[test]
fn ambiguous_no_auth_rotation_is_attempted_only_once_per_credential_key() {
    run("ambiguous", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut first, first_request) = request(&listener, deadline);
        let (mut second, second_request) = request(&listener, deadline);
        assert_request(&first_request, "GET", path, Some("stale-token"));
        assert_request(&second_request, "GET", path, Some("stale-token"));
        respond(
            &mut first,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        respond(
            &mut second,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        respond(
            &mut rotate,
            "409 Conflict",
            r#"{"error":"ambiguous_no_auth_tenant"}"#,
        );
        no_more_requests(&listener, Duration::from_millis(500));
    });
}

fn run_fence(mode: &'static str) {
    run(mode, move |listener, deadline, config| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut tasks, tasks_request) = request(&listener, deadline);
        assert_request(&tasks_request, "GET", path, Some("stale-token"));
        respond(
            &mut tasks,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        std::fs::write(config.join("no-auth-post-entered"), b"entered").unwrap();
        if mode == "logout-race" {
            let (mut logout, logout_request) = request(&listener, deadline);
            assert_request(
                &logout_request,
                "DELETE",
                "/api/v1/plugin/session",
                Some("stale-token"),
            );
            respond(&mut logout, "204 No Content", "");
        }
        let release = config.join("no-auth-post-release");
        while !release.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(release.exists(), "probe did not release no-auth response");
        respond(&mut rotate, "200 OK", &candidate("race-candidate"));
        let (mut revoke, revoke_request) = request(&listener, deadline);
        assert_request(
            &revoke_request,
            "DELETE",
            "/api/v1/plugin/session",
            Some("race-candidate"),
        );
        respond(&mut revoke, "204 No Content", "");
        no_more_requests(&listener, Duration::from_millis(250));
    });
}

#[test]
fn concurrent_logout_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("logout-race");
}

#[test]
fn concurrent_change_user_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("change-race");
}

#[test]
fn concurrent_config_change_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("config-race");
}

#[test]
fn persistence_preflight_failure_prevents_candidate_creation_and_retry() {
    run("persist-failure", |listener, _, _| {
        no_more_requests(&listener, Duration::from_millis(650));
    });
}

#[test]
fn post_preflight_persistence_failure_best_effort_revokes_the_candidate() {
    run(
        "post-preflight-persist-failure",
        |listener, deadline, config| {
            let (mut rotate, rotate_request) = request(&listener, deadline);
            assert_request(
                &rotate_request,
                "POST",
                "/api/v1/plugin/no-auth-session",
                None,
            );
            std::fs::remove_dir_all(&config).unwrap();
            std::fs::write(&config, b"block").unwrap();
            respond(&mut rotate, "200 OK", &candidate("persist-candidate"));
            let (mut revoke, revoke_request) = request(&listener, deadline);
            assert_request(
                &revoke_request,
                "DELETE",
                "/api/v1/plugin/session",
                Some("persist-candidate"),
            );
            respond(&mut revoke, "204 No Content", "");
            no_more_requests(&listener, Duration::from_millis(650));
        },
    );
}
