#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace BBL {

struct TaskQueryParams {
    std::string dev_id;
    int status = 0;
    int offset = 0;
    int limit = 20;
};

} // namespace BBL

namespace {

struct Library {
#if defined(_WIN32)
    HMODULE handle = nullptr;
#else
    void* handle = nullptr;
#endif

    explicit Library(const char* path) {
#if defined(_WIN32)
        handle = LoadLibraryA(path);
#else
        handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
    }

    ~Library() {
#if defined(_WIN32)
        if (handle) FreeLibrary(handle);
#else
        if (handle) dlclose(handle);
#endif
    }

    template <class T>
    T symbol(const char* name) const {
#if defined(_WIN32)
        auto* raw = handle ? reinterpret_cast<void*>(GetProcAddress(handle, name)) : nullptr;
#else
        auto* raw = handle ? dlsym(handle, name) : nullptr;
#endif
        if (!raw) {
            std::cerr << "missing symbol: " << name << '\n';
            std::exit(3);
        }
        return reinterpret_cast<T>(raw);
    }
};

[[noreturn]] void fail(
    void* agent,
    int (*destroy_agent)(void*),
    const std::string& message
) {
    if (agent) destroy_agent(agent);
    std::cerr << message << '\n';
    std::exit(2);
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: task_token_refresh_probe <plugin> <mode> <config-dir>\n";
        return 2;
    }
    Library library(argv[1]);
    using create_agent_fn = void* (*)(std::string);
    using destroy_agent_fn = int (*)(void*);
    using set_config_dir_fn = int (*)(void*, std::string);
    using get_tasks_fn = int (*)(void*, BBL::TaskQueryParams, std::string*);
    using get_plate_fn = int (*)(void*, std::string, int*);
    using get_subtask_fn =
        int (*)(void*, std::string, std::string*, unsigned int*, std::string*);
    auto create_agent = library.symbol<create_agent_fn>("bambu_network_create_agent");
    auto destroy_agent = library.symbol<destroy_agent_fn>("bambu_network_destroy_agent");
    auto set_config_dir = library.symbol<set_config_dir_fn>("bambu_network_set_config_dir");
    auto get_tasks = library.symbol<get_tasks_fn>("bambu_network_get_user_tasks");
    auto get_plate = library.symbol<get_plate_fn>("bambu_network_get_task_plate_index");
    auto get_subtask = library.symbol<get_subtask_fn>("bambu_network_get_subtask_info");

    void* agent = create_agent("task-token-refresh-probe");
    if (!agent) fail(agent, destroy_agent, "agent creation failed");
    const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
    if (!hub_url || hub_url[0] == '\0') {
        fail(agent, destroy_agent, "missing Hub URL for persisted no-auth session");
    }
    std::filesystem::create_directories(argv[3]);
    std::ofstream login(std::filesystem::path(argv[3]) / "pandar-plugin-login.json");
    login << "{\"hub_url\":\"" << hub_url
          << "\",\"token\":\"stale-token\",\"session_kind\":\"no_auth\","
             "\"profile\":{\"user_id\":\"stale-user\","
             "\"user_name\":\"Stale User\",\"tenant_id\":\"tenant-1\","
             "\"tenant_name\":\"Tenant\"}}";
    login.close();
    if (set_config_dir(agent, argv[3]) != 0) {
        fail(agent, destroy_agent, "set config dir failed");
    }
    const std::string mode = argv[2];
    if (mode == "tasks" || mode == "retry-rejected" || mode == "rotation-failure") {
        std::string body;
        const int result = get_tasks(agent, BBL::TaskQueryParams{}, &body);
        const bool expected = mode == "tasks"
            ? result == 0 && body == R"({"total":0,"hits":[]})"
            : mode == "retry-rejected"
                ? result == -19 && body.find("invalid_response") != std::string::npos
                : result == -19
                    && body.find("ambiguous_no_auth_tenant") != std::string::npos
                    && body.find("invalid_auth_token") == std::string::npos;
        if (!expected) {
            fail(agent, destroy_agent, "task list did not rotate and retry");
        }
    } else if (mode == "plate") {
        int plate_index = -1;
        if (get_plate(agent, "42", &plate_index) != 0 || plate_index != 3) {
            fail(agent, destroy_agent, "task plate did not rotate and retry");
        }
    } else if (mode == "subtask") {
        std::string task_json;
        std::string body;
        unsigned int http_code = 0;
        if (get_subtask(agent, "42", &task_json, &http_code, &body) != 0 ||
            http_code != 200 || task_json != body ||
            task_json.find("plate_idx") == std::string::npos) {
            fail(agent, destroy_agent, "subtask did not rotate and retry");
        }
    } else {
        fail(agent, destroy_agent, "unsupported mode");
    }
    destroy_agent(agent);
    std::cout << R"({"ok":true,"mode":")" << mode << R"("})" << '\n';
    return 0;
}
