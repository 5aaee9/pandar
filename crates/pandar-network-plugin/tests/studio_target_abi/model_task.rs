#[test]
fn model_subtask_abi_uses_a_typed_worker_and_studio_owned_target() {
    let content = include_str!("../../src/shim_abi_content.hpp");
    let model_task = include_str!("../../src/shim_model_task.hpp");
    let model_types = include_str!("../../src/shim_model_task_types.hpp");
    let print_types = include_str!("../../src/shim_print_types.hpp");
    let user = include_str!("../../src/shim_abi_user.hpp");
    let ffi = include_str!("../../src/studio_print/model_task.rs");
    let body = content
        .split_once("PANDAR_ABI int bambu_network_get_subtask(")
        .expect("model subtask ABI")
        .1
        .split_once("PANDAR_ABI int bambu_network_get_model_mall_home_url")
        .expect("model subtask ABI end")
        .0;
    assert!(body.contains("enqueue_model_task(current, task, std::move(callback))"));
    assert!(body.contains("BAMBU_NETWORK_SUCCESS"));
    assert!(body.contains("BAMBU_NETWORK_ERR_INVALID_RESULT"));
    assert!(!body.contains("callback("));
    assert!(
        ffi.contains("pub struct PluginStudioModelTask")
            && ffi.contains("#[serde(deny_unknown_fields)]")
            && ffi.contains("pandar_plugin_studio_get_model_task")
            && print_types.contains("struct PluginStudioModelTask")
            && print_types.contains("pandar_plugin_studio_get_model_task_with_session")
    );
    assert!(
        model_types.contains("class BBLModelTask")
            && model_types.contains("std::string profile_name")
            && model_task.contains("start_model_task_worker")
            && model_task.contains("stop_model_task_worker")
            && model_task.contains("callback(target)")
            && user.contains("start_model_task_worker(agent)")
            && user.contains("stop_model_task_worker(a)")
    );
    assert!(
        model_task.contains("callback_gate(agent->callback_mutex)")
            && model_task.contains("account_gate(agent->account_mutex)")
            && model_task.contains("if (model_task_worker_stopping(agent)) return")
    );
}
