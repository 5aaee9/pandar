#pragma once

#include <cstdint>
#include <string>

namespace pandar::network_plugin {

struct PrinterRequestSnapshot {
    std::string hub_url;
    std::string token;
    std::string printer_id;
    bool printer_authorized = false;
    bool account_transition_pending = false;
    std::uint64_t account_epoch = 0;
    std::uint64_t account_config_epoch = 0;
    std::uint64_t cache_generation = 0;
    std::uint64_t firmware_generation = 0;
    std::int32_t session_kind = 0;
};

} // namespace pandar::network_plugin
