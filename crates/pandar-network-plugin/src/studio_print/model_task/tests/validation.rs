use super::*;

#[test]
fn makerworld_or_incomplete_metadata_is_rejected_without_delivery() {
    let cases = [
        r#"{"job_id":41,"design_id":9,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":9,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":9,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"model-9","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":42,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"041","model_id":"","model_name":"Project","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":" ","profile_name":"Preset"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"\t"}"#,
        r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset","fallback":"fake"}"#,
    ];

    for body in cases {
        let (hub_url, server) = model_task_server(body);
        let mut state = SnapshotState {
            hub_url,
            token: "task-token".to_owned(),
            account_epoch: 7,
        };
        let account = PluginStudioAccount {
            snapshot: snapshot_for(&state),
            context: (&mut state as *mut SnapshotState).cast(),
            current_snapshot: Some(current_snapshot),
        };
        let mut captured = None;
        let result = pandar_plugin_studio_get_model_task(
            &account,
            bytes("41"),
            (&mut captured as *mut Option<CapturedTask>).cast(),
            Some(capture_task),
        );

        assert_eq!(result.status, 1, "accepted invalid response: {body}");
        assert_eq!(result.http_code, 502, "wrong status for: {body}");
        assert_eq!(captured, None, "delivered invalid response: {body}");
        free(result);
        server.join().expect("invalid response server joined");
    }
}
