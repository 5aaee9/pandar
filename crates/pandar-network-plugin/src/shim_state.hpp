#pragma once

#include "shim_connection.hpp"
#include "shim_account_ffi.hpp"
#include "shim_dispatch.hpp"

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
    if (pandar_plugin_firmware_session_generation_current(
            adapter->agent->firmware_session,
            adapter->observation.generation
        ) == 0) {
        // The account/config transition that bumped the generation also
        // invalidated this reserved observation; skip the stale handoff.
        return 0;
    }
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

std::string studio_dev_id(std::string dev_id) {
    if (const auto separator = dev_id.find('|'); separator != std::string::npos) {
        dev_id.resize(separator);
    }
    return dev_id;
}

std::string borrowed_string(const uint8_t* ptr, std::size_t len) {
    return std::string(reinterpret_cast<const char*>(ptr), len);
}


} // namespace pandar::network_plugin

#include "shim_printer_cache.hpp"
