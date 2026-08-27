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

} // namespace pandar::network_plugin
