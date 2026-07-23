#pragma once

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <filesystem>
#include <functional>
#include <memory>
#include <mutex>
#include <ostream>
#include <string>
#include <thread>

#include "pinned_consumer_hashes.hpp"
#include "pinned_model_task.hpp"

inline Slic3r::BBLModelTask::BBLModelTask()
{
    job_id = -1;
    design_id = -1;
    profile_id = -1;
}

namespace studio_model_task_consumer {

struct State {
    explicit State(const std::string& task_id)
    {
        task.job_id = -701;
        task.design_id = -702;
        task.profile_id = -703;
        task.instance_id = -704;
        task.task_id = task_id;
        task.model_id = "model-before";
        task.model_name = "name-before";
        task.profile_name = "profile-before";
    }

    ~State()
    {
        if (account_change.joinable()) account_change.join();
    }

    std::mutex mutex;
    std::condition_variable ready;
    int callbacks = 0;
    bool same_pointer = false;
    std::thread account_change;
    std::atomic<bool> account_change_done = false;
    std::atomic<int> account_change_rc = -999;
    bool account_change_returned_during_callback = false;
    Slic3r::BBLModelTask task;
};

struct Evidence {
    int rc;
    int null_task_rc;
    int empty_callback_rc;
    std::shared_ptr<State> state;
    long long destroy_elapsed_ms = -1;

    template<class Destroy>
    void destroy_agent(Destroy&& destroy)
    {
        const auto started = std::chrono::steady_clock::now();
        destroy();
        destroy_elapsed_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - started
        ).count();
    }

    template<class Escape>
    void write_json_fields(std::ostream& out, Escape&& escape) const
    {
        out << ",\"model_subtask_rc\":" << rc
            << ",\"model_task_status_hash\":\"" PANDAR_MODEL_TASK_STATUS_HASH "\""
            << ",\"model_task_layout_hash\":\"" PANDAR_MODEL_TASK_LAYOUT_HASH "\""
            << ",\"model_task_callback_hash\":\"" PANDAR_MODEL_TASK_CALLBACK_HASH "\""
            << ",\"model_task_forwarding_hash\":\"" PANDAR_MODEL_TASK_FORWARDING_HASH "\""
            << ",\"model_subtask_null_task_rc\":" << null_task_rc
            << ",\"model_subtask_empty_callback_rc\":" << empty_callback_rc
            << ",\"model_subtask_destroy_ms\":" << destroy_elapsed_ms
            << ",\"model_subtask_callbacks\":" << state->callbacks
            << ",\"model_subtask_same_pointer\":" << (state->same_pointer ? "true" : "false")
            << ",\"model_subtask_account_change_rc\":" << state->account_change_rc.load()
            << ",\"model_subtask_account_change_returned_during_callback\":"
            << (state->account_change_returned_during_callback ? "true" : "false")
            << ",\"model_subtask_job_id\":" << state->task.job_id
            << ",\"model_subtask_design_id\":" << state->task.design_id
            << ",\"model_subtask_profile_id\":" << state->task.profile_id
            << ",\"model_subtask_instance_id\":" << state->task.instance_id
            << ",\"model_subtask_task_id\":\"" << escape(state->task.task_id)
            << "\",\"model_subtask_model_id\":\"" << escape(state->task.model_id)
            << "\",\"model_subtask_model_name\":\"" << escape(state->task.model_name)
            << "\",\"model_subtask_profile_name\":\"" << escape(state->task.profile_name) << '"';
    }
};

inline bool expects_callback(const std::string& case_name, const std::string& task_id)
{
    return task_id == "38191" && case_name != "model_task_metadata_unavailable"
        && case_name != "model_task_invalid_2xx" && case_name != "stale_model_task"
        && case_name != "model_task_destroy_inflight"
        && case_name != "model_task_destroy_no_auth_recovery";
}

inline bool destroys_inflight(const std::string& case_name)
{
    return case_name == "model_task_destroy_inflight"
        || case_name == "model_task_destroy_no_auth_recovery";
}

template<class Invoke, class ChangeAccount>
Evidence invoke(
    Invoke&& invoke,
    const std::string& task_id,
    const std::string& case_name,
    const std::filesystem::path& race_directory,
    ChangeAccount&& change_account
)
{
    auto state = std::make_shared<State>(task_id);
    const int null_task_rc = invoke(
        static_cast<Slic3r::BBLModelTask*>(nullptr), [](Slic3r::BBLModelTask*) {}
    );
    Slic3r::BBLModelTask empty_callback_task;
    empty_callback_task.task_id = task_id;
    const int empty_callback_rc = invoke(
        &empty_callback_task, std::function<void(Slic3r::BBLModelTask*)>{}
    );
    const int rc = invoke(&state->task, [state, case_name, change_account](Slic3r::BBLModelTask* result) mutable {
        if (case_name == "model_task_callback_account_race") {
            state->account_change = std::thread([state, change_account = std::move(change_account)]() mutable {
                state->account_change_rc.store(change_account());
                state->account_change_done.store(true);
            });
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            state->account_change_returned_during_callback = state->account_change_done.load();
        }
        std::lock_guard<std::mutex> lock(state->mutex);
        ++state->callbacks;
        state->same_pointer = result == &state->task;
        state->ready.notify_all();
    });
    if (rc == 0 && expects_callback(case_name, task_id)) {
        std::unique_lock<std::mutex> lock(state->mutex);
        state->ready.wait_for(
            lock, std::chrono::seconds(5), [&] { return state->callbacks != 0; }
        );
    } else if (rc == 0 && !destroys_inflight(case_name)) {
        std::this_thread::sleep_for(std::chrono::milliseconds(500));
    }
    if (rc == 0 && destroys_inflight(case_name)) {
        const auto entered = race_directory / "request-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
        while (!std::filesystem::exists(entered) && std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
    }
    if (state->account_change.joinable()) state->account_change.join();
    return {rc, null_task_rc, empty_callback_rc, std::move(state)};
}

} // namespace studio_model_task_consumer
