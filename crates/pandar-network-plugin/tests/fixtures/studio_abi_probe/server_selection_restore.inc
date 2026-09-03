    if (server_selection_child_restore_mode) {
        // Fresh process with every plugin URL environment variable removed: the saved
        // server selection must be restored before the persisted login is evaluated.
        const char* expected_web = std::getenv("PANDAR_PROBE_EXPECTED_WEB_URL");
        const char* expected_hub = std::getenv("PANDAR_PROBE_EXPECTED_HUB_URL");
        if (!expected_web || !expected_hub) {
            fail(agent, destroy_agent, "restore child missing expected URL environment");
        }
        if (start(agent) != 0) {
            fail(agent, destroy_agent, "restore child agent start failed");
        }
        if (!is_user_login(agent)) {
            fail(agent, destroy_agent, "fresh process did not restore the persisted login");
        }
        if (!contains(build_login_cmd(agent), "probe-token")) {
            fail(agent, destroy_agent, "restored login envelope lacked the persisted token");
        }
        const std::string local_config = http_body(http_request(
            get_host(agent),
            "GET /config HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        ));
        if (json_field(local_config, "webUrl") != expected_web ||
            json_field(local_config, "hubUrl") != expected_hub) {
            fail(
                agent, destroy_agent,
                "restored local webserver did not serve the selected server: " + local_config
            );
        }
        destroy_agent(agent);
        return 0;
    }

    if (server_selection_override_mode) {
        // Explicit Hub URL environment configuration outranks the saved manual
        // selection, so the other-Hub credential must not be restored.
        if (is_user_login(agent)) {
            fail(agent, destroy_agent, "explicit Hub override restored a foreign-Hub login");
        }
        if (contains(build_login_cmd(agent), "probe-token")) {
            fail(agent, destroy_agent, "explicit Hub override exposed the old Hub token");
        }
        destroy_agent(agent);
        return 0;
    }

    if (server_selection_restore_mode) {
        const char* hub_env = std::getenv("PANDAR_PLUGIN_HUB_URL");
        const char* web_env = std::getenv("PANDAR_PLUGIN_FRONTEND_URL");
        if (!hub_env || hub_env[0] == '\0' || !web_env || web_env[0] == '\0') {
            fail(agent, destroy_agent, "server-selection parent missing URL environment");
        }
        if (start(agent) != 0) fail(agent, destroy_agent, "agent start failed");
        const std::string host = get_host(agent);

        // Manual selection through the local sign-in page, then a Studio ABI call
        // observes the selection so the plugin persists it.
        switch_local_hub_target(host, hub_env);
        (void)is_user_login(agent);

        unsigned int http_code = 0;
        std::string exchange_body;
        if (get_token(agent, "probe-ticket", &http_code, &exchange_body) != 0 ||
            http_code != 200) {
            fail(agent, destroy_agent, "ticket exchange after manual selection failed");
        }

        std::ifstream selection_stream(
            std::filesystem::path(config_dir) / "pandar-plugin-server-selection.json"
        );
        std::stringstream selection_body;
        selection_body << selection_stream.rdbuf();
        const std::string selection = selection_body.str();
        out.server_selection_persisted =
            contains(selection, "\"hub_url\":\"" + std::string(hub_env) + "\"") &&
            contains(selection, "\"web_url\":\"" + std::string(web_env) + "\"");
        if (!out.server_selection_persisted) {
            fail(
                agent, destroy_agent,
                "manual server selection was not persisted: " + selection
            );
        }

        const int restored_child = run_selection_child(
            argv[0], argv[1], argv[2], config_dir, "server-selection-restore-child",
            {
                {"PANDAR_PROBE_EXPECTED_WEB_URL", web_env},
                {"PANDAR_PROBE_EXPECTED_HUB_URL", hub_env},
            }
        );
        out.server_selection_restored = restored_child == 0;
        if (restored_child != 0) {
            fail(
                agent, destroy_agent,
                "fresh-process restore child failed with " + std::to_string(restored_child)
            );
        }

        const int override_child = run_selection_child(
            argv[0], argv[1], argv[2], config_dir, "server-selection-restore-override",
            {{"PANDAR_PLUGIN_HUB_URL", "https://override-hub.example.test"}}
        );
        out.server_selection_override_logged_out = override_child == 0;
        if (override_child != 0) {
            fail(
                agent, destroy_agent,
                "explicit-configuration override child failed with " +
                    std::to_string(override_child)
            );
        }

        destroy_agent(agent);
        print_json(out);
        return 0;
    }
