    if (request_admission_mode) {
        const std::string profile_a = R"({"token":"probe-token","user_id":"admission-a","user_name":"Admission A","tenant_id":"tenant-a","tenant_name":"Tenant A"})";
        const std::string profile_b = R"({"token":"probe-token","user_id":"admission-b","user_name":"Admission B","tenant_id":"tenant-b","tenant_name":"Tenant B"})";
        const std::string operation =
            R"({"print":{"command":"back_to_center","sequence_id":"admission-operation"}})";
        const std::string firmware =
            R"({"upgrade":{"command":"upgrade_confirm","sequence_id":"admission-firmware","src_id":1}})";
        if (change_user(agent, profile_a) != 0) {
            fail(agent, destroy_agent, "request admission account A setup failed");
        }
        std::string printers_body;
        unsigned int http_code = 0;
        if (get_print_info(agent, &http_code, &printers_body) != 0 || http_code != 200) {
            fail(agent, destroy_agent, "request admission printer seed failed");
        }
        const std::string get_version =
            R"({"info":{"command":"get_version","sequence_id":"admission-version"}})";
        std::atomic<int> version_callbacks{0};
        if (set_message(agent, [&](std::string, std::string body) {
                if (contains(body, R"("command":"get_version")")) ++version_callbacks;
            }) != 0 ||
            add_subscribe(agent, {
                "studio-serial-1",
                "unknown-studio-serial",
                "00000000-0000-0000-0000-000000000123",
            }) != 0) {
            fail(agent, destroy_agent, "request admission callback setup failed");
        }

        auto rejects_unauthorized = [&](const std::string& dev_id) {
            std::string catalog_body;
            unsigned int catalog_code = 0;
            const int operation_rc = send_cloud(agent, dev_id, operation, 0, 0);
            const int catalog_rc = get_printer_firmware(
                agent, dev_id, &catalog_code, &catalog_body
            );
            const int firmware_rc = send_cloud(agent, dev_id, firmware, 0, 0);
            const int version_rc = send_cloud(agent, dev_id, get_version, 0, 0);
            return operation_rc == kBambuInvalidResult &&
                catalog_rc == kBambuInvalidResult && catalog_code == 404 &&
                contains(catalog_body, "invalid_printer_id") &&
                firmware_rc == kBambuInvalidResult &&
                version_rc == kBambuConnectFailed;
        };
        if (!rejects_unauthorized("unknown-studio-serial") ||
            !rejects_unauthorized("00000000-0000-0000-0000-000000000123")) {
            fail(agent, destroy_agent, "unauthorized printer request was not rejected before Hub I/O");
        }

        std::string catalog_body;
        unsigned int catalog_code = 0;
        if (send_cloud(agent, "studio-serial-1", operation, 0, 0) != 0 ||
            get_printer_firmware(
                agent, "studio-serial-1", &catalog_code, &catalog_body
            ) != 0 || catalog_code != 200 ||
            send_cloud(agent, "studio-serial-1", firmware, 0, 0) != 0 ||
            send_cloud(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            version_callbacks.load() != 1) {
            fail(agent, destroy_agent, "authorized request admission changed the success path");
        }

        std::atomic<int> transition_callbacks{0};
        std::atomic<bool> transition_rejected{false};
        if (set_local_connect(agent, [&](int status, std::string, std::string) {
                if (status != 2) return;
                ++transition_callbacks;
                std::string transition_body;
                unsigned int transition_code = 0;
                const int operation_rc = send_cloud(
                    agent, "studio-serial-1", operation, 0, 0
                );
                const int catalog_rc = get_printer_firmware(
                    agent, "studio-serial-1", &transition_code, &transition_body
                );
                const int firmware_rc = send_cloud(
                    agent, "studio-serial-1", firmware, 0, 0
                );
                const int subscribe_rc = add_subscribe(agent, {"studio-serial-1"});
                const int version_rc = send_cloud(
                    agent, "studio-serial-1", get_version, 0, 0
                );
                transition_rejected = operation_rc == kBambuInvalidResult &&
                    catalog_rc == kBambuInvalidResult && transition_code == 409 &&
                    contains(transition_body, "account_transition") &&
                    firmware_rc == kBambuInvalidResult && subscribe_rc == 0 &&
                    version_rc == kBambuConnectFailed && version_callbacks.load() == 1;
            }) != 0 ||
            connect_printer(
                agent, "studio-serial-1", "127.0.0.1", "user", "pass", false
            ) != 0 ||
            change_user(agent, profile_b) != 0 ||
            transition_callbacks.load() != 1 || !transition_rejected.load()) {
            fail(agent, destroy_agent, "account transition admitted a printer request");
        }

        destroy_agent(agent);
        print_json(out);
        return 0;
    }
