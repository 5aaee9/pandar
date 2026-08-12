#pragma once

#include "shim_personal_preset_types.hpp"

namespace pandar::network_plugin {

inline PresetBytes preset_bytes(const std::string& value) {
    return {reinterpret_cast<const uint8_t*>(value.data()), value.size()};
}

struct PersonalPresetAccountCopy {
    std::string hub_url;
    std::string token;
    std::string user_id;
    std::uint64_t account_epoch = 0;
    std::uint64_t config_epoch = 0;
    std::int32_t session_kind = 0;
    std::int32_t transition_pending = 0;
    std::uint64_t identity = 0;

    PersonalPresetAccount view() const {
        return {preset_bytes(hub_url), preset_bytes(token), preset_bytes(user_id), account_epoch,
                config_epoch, session_kind, transition_pending, identity};
    }
};

inline PersonalPresetAccountCopy personal_preset_account(Agent* agent) {
    std::lock_guard<std::recursive_mutex> lock(agent->account_mutex);
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    const auto state = studio_session_state(agent);
    return {agent->hub_url, agent->token, agent->user_id, state.account_epoch,
            agent->account_config_epoch.load(std::memory_order_acquire),
            agent->account_session_kind, static_cast<int32_t>(state.account_transition_pending),
            agent->account_identity};
}

inline std::vector<PresetEntry> preset_entries(const std::map<std::string, std::string>& values) {
    std::vector<PresetEntry> entries;
    entries.reserve(values.size());
    for (const auto& [key, value] : values)
        entries.push_back({preset_bytes(key), preset_bytes(value)});
    return entries;
}

struct PresetListAdapter {
    Agent* agent;
    std::uint64_t account_epoch;
    std::uint64_t config_epoch;
    BBL::CheckFn check;
    BBL::ProgressFn progress;
    BBL::WasCancelledFn cancel;
};

inline int32_t preset_check(void* context, const PresetEntry* entries, std::size_t count) {
    auto& adapter = *static_cast<PresetListAdapter*>(context);
    std::map<std::string, std::string> values;
    for (std::size_t index = 0; index < count; ++index) {
        values.emplace(
            std::string(reinterpret_cast<const char*>(entries[index].key.ptr), entries[index].key.len),
            std::string(reinterpret_cast<const char*>(entries[index].value.ptr), entries[index].value.len)
        );
    }
    return !adapter.check || adapter.check(std::move(values));
}

inline int32_t preset_progress(void* context, int32_t value) {
    auto& adapter = *static_cast<PresetListAdapter*>(context);
    if (adapter.progress) adapter.progress(value);
    return 0;
}

inline int32_t preset_cancel(void* context, int32_t) {
    auto& adapter = *static_cast<PresetListAdapter*>(context);
    return adapter.cancel && adapter.cancel();
}

inline int32_t preset_current(void* context, int32_t) {
    auto& adapter = *static_cast<PresetListAdapter*>(context);
    if (!adapter.agent) return 0;
    std::lock_guard<std::recursive_mutex> account(adapter.agent->account_mutex);
    const auto state = studio_session_state(adapter.agent);
    return !state.account_transition_pending && state.account_epoch == adapter.account_epoch &&
        adapter.agent->account_config_epoch.load(std::memory_order_acquire) == adapter.config_epoch;
}

struct PresetDrainAdapter {
    std::map<std::string, std::map<std::string, std::string>>* output;
};

inline int32_t preset_drain(void* context, PresetBytes first, PresetBytes second) {
    auto& adapter = *static_cast<PresetDrainAdapter*>(context);
    std::string name(reinterpret_cast<const char*>(first.ptr), first.len);
    auto value = std::string(reinterpret_cast<const char*>(second.ptr), second.len);
    auto separator = name.find('\0');
    if (separator == std::string::npos) return 1;
    (*adapter.output)[name.substr(0, separator)][name.substr(separator + 1)] = std::move(value);
    return 0;
}

inline PresetResult preset_mutate(
    Agent* agent, int operation, const std::string& id, const std::string& name,
    std::map<std::string, std::string>* values
) {
    auto account_copy = personal_preset_account(agent);
    auto account = account_copy.view();
    auto entries = values ? preset_entries(*values) : std::vector<PresetEntry>{};
    return pandar_plugin_personal_preset_mutate(
        operation, &account, preset_bytes(id), preset_bytes(name), entries.data(), entries.size()
    );
}

inline std::string take_preset_id(PresetResult& result) {
    std::string id;
    if (result.id_ptr && result.id_len)
        id.assign(reinterpret_cast<const char*>(result.id_ptr), result.id_len);
    pandar_plugin_free_with_capacity(result.id_ptr, result.id_len, result.id_cap);
    result.id_ptr = nullptr;
    return id;
}

inline std::string preset_create(
    Agent* agent, const std::string& name, std::map<std::string, std::string>* values,
    unsigned int* http_code
) {
    if (!agent) {
        if (http_code) *http_code = 0;
        return {};
    }
    if (!values) {
        if (http_code) *http_code = 400;
        return {};
    }
    auto response = preset_mutate(agent, 1, {}, name, values);
    if (http_code) *http_code = response.http_code;
    if (response.updated_time > 0)
        (*values)["updated_time"] = std::to_string(response.updated_time);
    if (response.code > 0) (*values)["code"] = std::to_string(response.code);
    return take_preset_id(response);
}

} // namespace pandar::network_plugin
