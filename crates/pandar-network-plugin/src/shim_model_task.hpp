#pragma once

#include "shim_tasks.hpp"

namespace pandar::network_plugin {

struct ModelTaskAdapterState {
    bool filled = false;
    std::int32_t job_id = 0;
    std::int32_t design_id = 0;
    std::int32_t profile_id = 0;
    std::int32_t instance_id = 0;
    std::string task_id;
    std::string model_id;
    std::string model_name;
    std::string profile_name;
};

inline bool copy_model_task_text(PluginBytes source, std::string& target) {
    if (source.len == 0) {
        target.clear();
        return true;
    }
    if (source.len > 0 && !source.ptr) return false;
    target.assign(reinterpret_cast<const char*>(source.ptr), source.len);
    return true;
}

inline std::int32_t collect_model_task(
    void* opaque,
    const PluginStudioModelTask* source
) {
    auto* target = static_cast<ModelTaskAdapterState*>(opaque);
    if (!target || !source || target->filled) return 0;
    if (!copy_model_task_text(source->task_id, target->task_id) ||
        !copy_model_task_text(source->model_id, target->model_id) ||
        !copy_model_task_text(source->model_name, target->model_name) ||
        !copy_model_task_text(source->profile_name, target->profile_name)) return 0;
    target->job_id = source->job_id;
    target->design_id = source->design_id;
    target->profile_id = source->profile_id;
    target->instance_id = source->instance_id;
    target->filled = true;
    return 1;
}

inline bool model_task_worker_stopping(Agent* agent) {
    std::lock_guard<std::mutex> worker(agent->model_task_mutex);
    return agent->model_task_stop;
}

inline std::int32_t model_task_request_cancelled(void* opaque) {
    return model_task_worker_stopping(static_cast<Agent*>(opaque)) ? 1 : 0;
}

inline void run_model_task_request(
    Agent* agent,
    std::string requested_task_id,
    Slic3r::BBLModelTask* target,
    std::function<void(Slic3r::BBLModelTask*)> callback
) {
    trace_plugin_event(agent, "model-task request started");
    refresh_local_webserver_config(agent);
    const auto initial_snapshot = printer_request_snapshot(agent, {});
    StudioAccountSnapshotContext account_context{agent, {}};
    const auto account = studio_account(initial_snapshot, account_context);
    ModelTaskAdapterState adapter;
    auto result = pandar_plugin_studio_get_model_task_with_session(
        agent->printer_refresh_session,
        &account,
        initial_snapshot.account_config_epoch,
        initial_snapshot.session_kind,
        agent,
        with_current_account,
        plugin_bytes(requested_task_id),
        &adapter,
        collect_model_task,
        agent,
        model_task_request_cancelled
    );
    const auto status = result.status;
    body_from_result(result);
    if (status != 0 || !adapter.filled) return;
    trace_plugin_event(agent, "model-task response accepted");

    std::lock_guard<std::recursive_timed_mutex> callback_gate(agent->callback_mutex);
    std::lock_guard<std::recursive_mutex> account_gate(agent->account_mutex);
    if (model_task_worker_stopping(agent)) return;
    const auto current_snapshot = printer_request_snapshot(agent, {});
    const auto expected = plugin_studio_snapshot(account_context.current_snapshot);
    const auto current = plugin_studio_snapshot(current_snapshot);
    if (pandar_plugin_studio_request_snapshot_current(&expected, &current) == 0 ||
        !printer_request_snapshot_current(agent, account_context.current_snapshot)) return;

    target->job_id = adapter.job_id;
    target->design_id = adapter.design_id;
    target->profile_id = adapter.profile_id;
    target->instance_id = adapter.instance_id;
    target->task_id = std::move(adapter.task_id);
    target->model_id = std::move(adapter.model_id);
    target->model_name = std::move(adapter.model_name);
    target->profile_name = std::move(adapter.profile_name);
    trace_plugin_event(agent, "model-task callback started");
    callback(target);
    trace_plugin_event(agent, "model-task callback returned");
}

inline void start_model_task_worker(Agent* agent) {
    if (!agent || agent->model_task_thread.joinable()) return;
    {
        std::lock_guard<std::mutex> worker(agent->model_task_mutex);
        agent->model_task_stop = false;
        agent->model_task_busy = false;
        agent->model_task_job = {};
    }
    agent->model_task_thread = std::thread([agent] {
        while (true) {
            std::function<void()> job;
            {
                std::unique_lock<std::mutex> worker(agent->model_task_mutex);
                agent->model_task_wake.wait(worker, [agent] {
                    return agent->model_task_stop || static_cast<bool>(agent->model_task_job);
                });
                if (agent->model_task_stop) {
                    agent->model_task_job = {};
                    agent->model_task_busy = false;
                    return;
                }
                job = std::move(agent->model_task_job);
                agent->model_task_job = {};
            }
            job();
            std::lock_guard<std::mutex> worker(agent->model_task_mutex);
            agent->model_task_busy = false;
        }
    });
}

inline void stop_model_task_worker(Agent* agent) {
    if (!agent) return;
    {
        std::lock_guard<std::recursive_timed_mutex> callback_gate(agent->callback_mutex);
        std::lock_guard<std::mutex> worker(agent->model_task_mutex);
        agent->model_task_stop = true;
        agent->model_task_job = {};
    }
    agent->model_task_wake.notify_all();
    if (agent->model_task_thread.joinable()) agent->model_task_thread.join();
}

inline bool enqueue_model_task(
    Agent* agent,
    Slic3r::BBLModelTask* task,
    std::function<void(Slic3r::BBLModelTask*)> callback
) {
    if (!agent || !task || !callback || task->task_id.empty()) return false;
    std::lock_guard<std::mutex> worker(agent->model_task_mutex);
    if (agent->model_task_stop || agent->model_task_busy ||
        !agent->model_task_thread.joinable()) return false;
    const auto task_id = task->task_id;
    agent->model_task_busy = true;
    agent->model_task_job = [agent, task_id, task, callback = std::move(callback)]() mutable {
        run_model_task_request(agent, task_id, task, std::move(callback));
    };
    agent->model_task_wake.notify_one();
    return true;
}

} // namespace pandar::network_plugin
