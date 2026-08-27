use super::*;

pub(super) fn response_for(
    first: &str,
    job_polls: &AtomicUsize,
    case: &str,
) -> (&'static str, String) {
    if first.starts_with("GET /api/v1/plugin/printers ") {
        return ("HTTP/1.1 200 OK", printer_response().to_owned());
    }
    if first.starts_with("POST /api/v1/plugin/prints ") {
        return (
            "HTTP/1.1 201 Created",
            r#"{"task_id":38191,"studio_submission_id":38191,"status":"queued"}"#.to_owned(),
        );
    }
    if first.starts_with("POST /api/v1/plugin/jobs/38191/cancel ") {
        return cancel_response(case);
    }
    if first.starts_with("GET /api/v1/plugin/jobs/38191/model-task ") {
        return model_task_response(case);
    }
    if first.contains("/plate") {
        if case == "task_unknown" {
            return (
                "HTTP/1.1 404 Not Found",
                r#"{"error":"job_not_found"}"#.to_owned(),
            );
        }
        if case == "task_invalid_plate_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"plate_index":0}"#.to_owned(),
            );
        }
        return (
            "HTTP/1.1 200 OK",
            r#"{"studio_submission_id":38191,"plate_index":7}"#.to_owned(),
        );
    }
    if first.contains("/subtask") {
        if case == "task_unknown" {
            return (
                "HTTP/1.1 404 Not Found",
                r#"{"error":"job_not_found"}"#.to_owned(),
            );
        }
        if case == "task_metadata_unavailable" {
            return (
                "HTTP/1.1 409 Conflict",
                r#"{"error":"studio_task_metadata_unavailable"}"#.to_owned(),
            );
        }
        if case == "task_invalid_subtask_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_oversized_subtask_weight_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":1.7976931348623157e308,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_oversized_subtask_prediction_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":2147483648,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_nonpositive_subtask_plate_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":0}}","context":{"plates":[{"index":0,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_mixed_invalid_subtask_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]},{"index":-1,"thumbnail":{"url":"https://untrusted.invalid/private.png"},"prediction":2147483648,"weight":12.5,"filaments":[]}]}}"##.to_owned(),
            );
        }
        return ("HTTP/1.1 200 OK", subtask_response().to_owned());
    }
    if first.starts_with("GET /api/v1/plugin/jobs?")
        || first.starts_with("GET /api/v1/plugin/jobs ")
    {
        if case == "task_hub_outage" {
            return (
                "HTTP/1.1 503 Service Unavailable",
                r#"{"error":"hub_unavailable"}"#.to_owned(),
            );
        }
        if case == "task_sensitive_page_2xx" {
            return (
                "HTTP/1.1 200 OK",
                format!(r#"{{"total":"{DIAGNOSTIC_SECRET}","hits":[]}}"#),
            );
        }
        if case == "task_sensitive_error_4xx" {
            return (
                "HTTP/1.1 400 Bad Request",
                format!(r#"{{"error":{{"detail":"{DIAGNOSTIC_SECRET}"}}}}"#),
            );
        }
        if case == "task_oversized_total_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"total":2147483648,"hits":[]}"#.to_owned(),
            );
        }
        if case == "task_ambiguous_title_2xx" {
            return (
                "HTTP/1.1 200 OK",
                task_page_response().replace(
                    r#""title":"contract-base.3mf""#,
                    r#""title":"contract-base.3mf","designTitle":"ambiguous""#,
                ),
            );
        }
        if case == "task_nonempty_cover_2xx" {
            return (
                "HTTP/1.1 200 OK",
                task_page_response().replace(
                    r#""cover":"""#,
                    r#""cover":"https://untrusted.invalid/private.png""#,
                ),
            );
        }
        return ("HTTP/1.1 200 OK", task_page_response().to_owned());
    }
    if first.starts_with("GET /api/v1/plugin/jobs/38191 ") {
        if case == "lifecycle_hub_outage" {
            return (
                "HTTP/1.1 503 Service Unavailable",
                r#"{"error":"hub_unavailable"}"#.to_owned(),
            );
        }
        if case == "lifecycle_sensitive_page_2xx" {
            return (
                "HTTP/1.1 200 OK",
                format!(
                    r#"{{"studio_submission_id":"{DIAGNOSTIC_SECRET}","job_status":"queued","print_status":"pending"}}"#
                ),
            );
        }
        if case == "lifecycle_sensitive_error_4xx" {
            return (
                "HTTP/1.1 400 Bad Request",
                format!(r#"{{"error":{{"detail":"{DIAGNOSTIC_SECRET}"}}}}"#),
            );
        }
        let poll = job_polls.fetch_add(1, Ordering::SeqCst);
        if case == "downstream_failure" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"job_status":"failed","print_status":"pending","failure":{"phase":"data_connection","cause":"start protected upload: 522 SSL connection failed: session reuse required"}}"#.to_owned(),
            );
        }
        if case == "generic_downstream_failure" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"job_status":"failed","print_status":"pending"}"#
                    .to_owned(),
            );
        }
        let (state, print_status) = if poll == 0 {
            ("acknowledged", "pending")
        } else if case == "physical_abort_after_publish" {
            ("succeeded", "cancelled")
        } else {
            ("succeeded", "pending")
        };
        return (
            "HTTP/1.1 200 OK",
            format!(
                r#"{{"studio_submission_id":38191,"job_status":"{state}","print_status":"{print_status}"}}"#
            ),
        );
    }
    (
        "HTTP/1.1 404 Not Found",
        r#"{"error":"contract_route_not_found"}"#.to_owned(),
    )
}

fn model_task_response(case: &str) -> (&'static str, String) {
    if case == "model_task_destroy_no_auth_recovery" {
        return (
            "HTTP/1.1 401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#.to_owned(),
        );
    }
    if case == "model_task_metadata_unavailable" {
        return (
            "HTTP/1.1 409 Conflict",
            r#"{"error":"studio_model_task_metadata_unavailable"}"#.to_owned(),
        );
    }
    if case == "model_task_invalid_2xx" {
        return (
            "HTTP/1.1 200 OK",
            r#"{"job_id":38191,"design_id":44,"profile_id":55,"instance_id":38191,"task_id":"38192","model_id":"foreign-model","model_name":"foreign-project","profile_name":"foreign-preset"}"#.to_owned(),
        );
    }
    (
        "HTTP/1.1 200 OK",
        r#"{"job_id":38191,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"38191","model_id":"","model_name":"contract-base-project","profile_name":"contract-base-preset"}"#
            .to_owned(),
    )
}

fn cancel_response(case: &str) -> (&'static str, String) {
    if case == "cancel_too_late" || case == "cancel_after_wait" {
        return (
            "HTTP/1.1 409 Conflict",
            r#"{"error":"cancel_too_late"}"#.to_owned(),
        );
    }
    if case == "cancel_wrong_id" {
        return (
            "HTTP/1.1 200 OK",
            r#"{"studio_submission_id":38192,"job_status":"cancelled","print_status":"cancelled"}"#
                .to_owned(),
        );
    }
    if case == "stale_after_201" || case == "stale_cancel_failed" || case == "cancel_race_stale" {
        return (
            "HTTP/1.1 503 Service Unavailable",
            r#"{"error":"hub_unavailable"}"#.to_owned(),
        );
    }
    (
        "HTTP/1.1 200 OK",
        r#"{"studio_submission_id":38191,"job_status":"cancelled","print_status":"cancelled"}"#
            .to_owned(),
    )
}
