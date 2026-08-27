#include "studio_abi_probe/part_01.inc"
#include "studio_abi_probe/part_02.inc"
#include "../../src/shim_firmware_request.hpp"

using namespace pandar::network_plugin;

using session_create_fn = void* (*)(
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    std::uint64_t
);
using session_update_fn = std::int32_t (*)(
    void*,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    std::uint64_t
);
using catalog_fn = PluginHttpResult (*)(
    void*,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    std::uint64_t
);
using refresh_fn = PluginHttpResult (*)(
    void*,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    std::uint64_t
);
using send_fn = PluginHttpResult (*)(
    void*,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    const std::uint8_t*, std::size_t,
    std::int32_t,
    std::uint64_t*,
    std::uint64_t
);
using free_fn = void (*)(void*, std::size_t, std::size_t);
using destroy_fn = void (*)(void*);

const std::uint8_t* bytes(const std::string& value) {
    return reinterpret_cast<const std::uint8_t*>(value.data());
}

void consume(PluginHttpResult result, free_fn free_body) {
    free_body(result.body_ptr, result.body_len, result.body_cap);
}

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: firmware_snapshot_claim_probe <plugin> <hub-a> <hub-b>\n";
        return 2;
    }
    Library library(argv[1]);
    if (!library.ok()) {
        std::cerr << "failed to load plugin library\n";
        return 3;
    }

    auto create = library.sym<session_create_fn>("pandar_plugin_firmware_session_create");
    auto update = library.sym<session_update_fn>("pandar_plugin_firmware_session_update");
    auto catalog = library.sym<catalog_fn>("pandar_plugin_firmware_catalog");
    auto refresh = library.sym<refresh_fn>("pandar_plugin_firmware_refresh_version");
    auto send = library.sym<send_fn>("pandar_plugin_firmware_send");
    auto free_body = library.sym<free_fn>("pandar_plugin_free_with_capacity");
    auto destroy = library.sym<destroy_fn>("pandar_plugin_firmware_session_destroy");

    const std::string hub_a = argv[2];
    const std::string hub_b = argv[3];
    const std::string token_a = "token-a";
    const std::string token_b = "token-b";
    void* session = create(
        bytes(hub_a), hub_a.size(), bytes(token_a), token_a.size(), 1
    );
    if (!session) {
        std::cerr << "failed to create generation A session\n";
        return 4;
    }

    PrinterRequestSnapshot snapshot;
    snapshot.printer_id = "printer-a";
    snapshot.firmware_generation = 1;
    if (update(
            session,
            bytes(hub_b), hub_b.size(), bytes(token_b), token_b.size(), 2
        ) != 0) {
        destroy(session);
        std::cerr << "failed to rotate session to generation B\n";
        return 5;
    }

    const std::string studio_dev_id = "studio-a";
    const std::string sequence_id = "snapshot-claim";
    const std::string message =
        R"({"upgrade":{"command":"upgrade_confirm","sequence_id":"snapshot-claim","src_id":1}})";
    auto catalog_result = firmware_catalog_from_snapshot(
        catalog, session, studio_dev_id, snapshot
    );
    auto refresh_result = refresh(
        session,
        bytes(studio_dev_id), studio_dev_id.size(),
        bytes(snapshot.printer_id), snapshot.printer_id.size(),
        bytes(sequence_id), sequence_id.size(),
        snapshot.firmware_generation
    );
    std::uint64_t callback_token = 99;
    auto send_result = send(
        session,
        bytes(studio_dev_id), studio_dev_id.size(),
        bytes(snapshot.printer_id), snapshot.printer_id.size(),
        bytes(message), message.size(),
        0,
        &callback_token,
        snapshot.firmware_generation
    );

    const bool exact = catalog_result.status == 1 &&
        refresh_result.status == 0 && send_result.status == 1 && callback_token == 0;
    consume(catalog_result, free_body);
    consume(refresh_result, free_body);
    consume(send_result, free_body);
    destroy(session);
    if (!exact) {
        std::cerr << "stale snapshot was not rejected at all firmware claims\n";
        return 6;
    }
    std::cout << "{\"ok\":true,\"send_token\":0}\n";
    return 0;
}
