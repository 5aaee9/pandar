#pragma once

#include "shim_request_types.hpp"

#include <cstddef>
#include <cstdint>
#include <string>

namespace pandar::network_plugin {

template <typename Catalog>
auto firmware_catalog_from_snapshot(
    Catalog catalog,
    void* session,
    const std::string& studio_dev_id,
    const PrinterRequestSnapshot& snapshot
) {
    return catalog(
        session,
        reinterpret_cast<const std::uint8_t*>(studio_dev_id.data()), studio_dev_id.size(),
        reinterpret_cast<const std::uint8_t*>(snapshot.printer_id.data()), snapshot.printer_id.size(),
        snapshot.firmware_generation
    );
}

template <typename RefreshVersion>
auto firmware_version_from_snapshot(
    RefreshVersion refresh_version,
    void* session,
    const std::string& studio_dev_id,
    const std::string& sequence_id,
    const PrinterRequestSnapshot& snapshot
) {
    return refresh_version(
        session,
        reinterpret_cast<const std::uint8_t*>(studio_dev_id.data()), studio_dev_id.size(),
        reinterpret_cast<const std::uint8_t*>(snapshot.printer_id.data()), snapshot.printer_id.size(),
        reinterpret_cast<const std::uint8_t*>(sequence_id.data()), sequence_id.size(),
        snapshot.firmware_generation
    );
}

template <typename Send>
auto firmware_send_from_snapshot(
    Send send,
    void* session,
    const std::string& studio_dev_id,
    const std::string& message,
    std::int32_t tunnel,
    std::uint64_t* callback_token,
    const PrinterRequestSnapshot& snapshot
) {
    return send(
        session,
        reinterpret_cast<const std::uint8_t*>(studio_dev_id.data()), studio_dev_id.size(),
        reinterpret_cast<const std::uint8_t*>(snapshot.printer_id.data()), snapshot.printer_id.size(),
        reinterpret_cast<const std::uint8_t*>(message.data()), message.size(),
        tunnel,
        callback_token,
        snapshot.firmware_generation
    );
}

} // namespace pandar::network_plugin
