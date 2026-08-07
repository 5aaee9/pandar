#pragma once

#include "shim_connection.hpp"
#include "shim_account_ffi.hpp"
#include "shim_account_callbacks.hpp"

namespace pandar::network_plugin {

using PluginPrinterRefreshTransaction = std::int32_t (*)(void*);
using PluginPrinterRefreshWithLock = std::int32_t (*)(
    void*, void*, PluginPrinterRefreshTransaction
);
using PluginPrinterRefreshFirmwareTransaction = std::int32_t (*)(
    void*, void*, std::uint64_t, std::uint64_t
);
using PluginPrinterRefreshWithFirmware = std::int32_t (*)(
    void*, void*, PluginPrinterRefreshFirmwareTransaction
);

struct PluginPrinterRefreshAdapter {
    void* context;
    PluginPrinterRefreshWithLock with_refresh_lock;
    void (*reserve_observation)(void*);
    PluginPrinterRefreshWithFirmware with_firmware_observation;
    PluginConnectionDeviceVisitor collect_offline;
};

struct PluginPrinterRefreshLifecycleResult {
    PluginHttpResult http;
    PluginConnectionResult connection;
    std::int32_t cache_committed;
    std::int32_t snapshot_current;
};

constexpr std::int32_t kPrinterRefreshStudioPrintInfo = 1;
constexpr std::int32_t kPrinterRefreshBackground = 2;

extern "C" PluginPrinterRefreshLifecycleResult pandar_plugin_printer_refresh_with_session(
    void*, std::int32_t, void*, PluginWithCurrentAccount, PluginPrinterRefreshAdapter
);

Agent* as_agent(void* raw) {
    return reinterpret_cast<Agent*>(raw);
}

void trace_plugin_event(const Agent* agent, const std::string& message) {
    if (!agent) return;
    std::lock_guard<std::mutex> lock(agent->trace_mutex);
    std::filesystem::path base;
    base = !agent->config_dir.empty() ? std::filesystem::path(agent->config_dir)
                                      : std::filesystem::path(agent->log_dir);
    if (base.empty()) return;
    std::error_code ec;
    std::filesystem::create_directories(base, ec);
    std::ofstream out(base / "pandar-network-plugin.trace.log", std::ios::app);
    if (out) out << message << '\n';
}

void trace_plugin_event(const Agent* agent, const std::string& event, const std::string& dev_id) {
    trace_plugin_event(agent, event + " dev_id=" + dev_id);
}

std::string body_from_result(PluginHttpResult result);
void refresh_local_webserver_config(Agent* agent);
void sync_printer_refresh_session(Agent* agent);
void invalidate_firmware_account_session(Agent* agent);
bool try_no_auth_session(Agent* agent, bool initial_attempt);
FirmwareObservationTicket begin_firmware_observation(Agent* agent);
struct PrinterRefreshAdapterState {
    Agent* agent;
    FirmwareObservationTicket observation;
    std::vector<IssuedOfflineDelivery> offline;
};

extern "C" std::int32_t with_printer_refresh_lock(
    void* context,
    void* transaction_context,
    PluginPrinterRefreshTransaction transaction
) noexcept {
    auto* adapter = static_cast<PrinterRefreshAdapterState*>(context);
    if (!adapter || !adapter->agent || !transaction) return 1;
    std::lock_guard<std::recursive_mutex> refresh(adapter->agent->printer_refresh_mutex);
    return transaction(transaction_context);
}

extern "C" void reserve_printer_refresh_observation(void* context) noexcept {
    auto* adapter = static_cast<PrinterRefreshAdapterState*>(context);
    adapter->observation = begin_firmware_observation(adapter->agent);
}

extern "C" std::int32_t with_printer_refresh_firmware(
    void* context,
    void* projection_context,
    PluginPrinterRefreshFirmwareTransaction transaction
) noexcept {
    auto* adapter = static_cast<PrinterRefreshAdapterState*>(context);
    if (!adapter || !adapter->agent || !adapter->agent->firmware_session || !transaction) return 0;
    std::lock_guard<std::recursive_mutex> transition(
        adapter->agent->firmware_transition_mutex
    );
    return transaction(
        projection_context,
        adapter->agent->firmware_session,
        adapter->observation.generation,
        adapter->observation.sequence
    );
}

extern "C" void collect_printer_refresh_offline(
    void* context,
    const std::uint8_t* dev_id,
    std::size_t dev_id_len,
    std::uint64_t ticket
) noexcept {
    auto* adapter = static_cast<PrinterRefreshAdapterState*>(context);
    adapter->offline.push_back({
        std::string(reinterpret_cast<const char*>(dev_id), dev_id_len),
        ticket,
    });
}

PluginPrinterRefreshAdapter printer_refresh_adapter(PrinterRefreshAdapterState* state) {
    return {
        state,
        with_printer_refresh_lock,
        reserve_printer_refresh_observation,
        with_printer_refresh_firmware,
        collect_printer_refresh_offline,
    };
}

struct LocalLostDelivery {
    std::uint64_t account_epoch = 0;
};

LocalLostDelivery reset_account_printer_state(Agent* agent) {
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    pandar_plugin_studio_begin_account_transition(agent->printer_refresh_session);
    invalidate_firmware_account_session(agent);
    return {studio_session_state(agent).account_epoch};
}

std::function<void()> finish_account_printer_transition(
    Agent* agent,
    const LocalLostDelivery& transition
) {
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    auto offline = take_printer_offline_transitions(agent);
    return [agent, account_epoch = transition.account_epoch,
            offline = std::move(offline)]() mutable {
        dispatch_printer_offline_transitions(agent, offline);
        std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
        pandar_plugin_studio_finish_account_transition(
            agent->printer_refresh_session, account_epoch
        );
    };
}

std::string account_token_snapshot(const Agent* agent) {
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    return agent->token;
}

bool account_session_current(
    const Agent* agent,
    std::uint64_t expected_epoch,
    const std::string& expected_token,
    bool require_logged_out
) {
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    const auto state = studio_session_state(agent);
    return static_cast<AccountPolicyAction>(pandar_plugin_account_commit_action(
        expected_epoch,
        state.account_epoch,
        reinterpret_cast<const uint8_t*>(expected_token.data()), expected_token.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()), agent->token.size(),
        require_logged_out
    )) == AccountPolicyAction::Apply;
}

bool account_transition_current(
    const Agent* agent,
    const LocalLostDelivery& transition,
    const std::string& expected_token,
    bool require_logged_out
) {
    return account_session_current(
        agent, transition.account_epoch, expected_token, require_logged_out
    );
}

LocalLostDelivery clear_login_state(Agent* agent, bool sync_sessions = true) {
    auto lost = reset_account_printer_state(agent);
    std::unique_lock<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    agent->token.clear();
    agent->user_id.clear();
    agent->user_name.clear();
    agent->avatar.clear();
    agent->profile_json.clear();
    agent->account_session_kind = 0;
    if (sync_sessions) {
        sync_printer_refresh_session(agent);
    }
    return lost;
}

std::string studio_dev_id(std::string dev_id) {
    if (const auto separator = dev_id.find('|'); separator != std::string::npos) {
        dev_id.resize(separator);
    }
    return dev_id;
}

std::string borrowed_string(const uint8_t* ptr, std::size_t len) {
    return std::string(reinterpret_cast<const char*>(ptr), len);
}

bool refresh_printer_status_cache(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return false;
    refresh_local_webserver_config(agent);
    std::unique_lock<std::mutex> request(agent->printer_refresh_request_mutex);
    PrinterRefreshAdapterState adapter_state{agent};
    auto lifecycle = pandar_plugin_printer_refresh_with_session(
        agent->printer_refresh_session,
        kPrinterRefreshBackground,
        agent,
        with_current_account,
        printer_refresh_adapter(&adapter_state)
    );
    const auto status = lifecycle.http.status;
    const auto http_code = lifecycle.http.http_code;
    auto body = body_from_result(lifecycle.http);
    const auto cache_committed = lifecycle.cache_committed != 0;
    request.unlock();
    dispatch_connection_transition(agent, lifecycle.connection);
    dispatch_printer_offline_transitions(agent, std::move(adapter_state.offline));
    if (!cache_committed) {
        trace_plugin_event(
            agent,
            "printer status refresh failed status=" + std::to_string(status) +
                " http_code=" + std::to_string(http_code) + " body=" + body
        );
    }
    return cache_committed;
}

} // namespace pandar::network_plugin

#include "shim_printer_cache.hpp"
