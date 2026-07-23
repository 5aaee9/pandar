#include <atomic>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <string>
#include <thread>

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
}

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
    template <class T> T symbol(const char* name) const {
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

void wait_for(const std::filesystem::path& path) {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
    while (!std::filesystem::exists(path) && std::chrono::steady_clock::now() < deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    if (!std::filesystem::exists(path)) {
        std::cerr << "timed out waiting for " << path.filename().string() << '\n';
        std::exit(2);
    }
}

[[noreturn]] void fail(void* agent, int (*destroy)(void*), const std::string& message) {
    if (agent) destroy(agent);
    std::cerr << message << '\n';
    std::exit(2);
}
}

int main(int argc, char** argv) {
    if (argc != 4) return 2;
    Library library(argv[1]);
    using create_fn = void* (*)(std::string);
    using destroy_fn = int (*)(void*);
    using config_fn = int (*)(void*, std::string);
    using change_fn = int (*)(void*, std::string);
    using logout_fn = int (*)(void*, bool);
    using start_fn = int (*)(void*);
    using logged_in_fn = bool (*)(void*);
    using get_name_fn = std::string (*)(void*);
    using login_cmd_fn = std::string (*)(void*);
    using tasks_fn = int (*)(void*, BBL::TaskQueryParams, std::string*);
    using login_callback_fn = int (*)(void*, std::function<void(int, bool)>);
    using printer_callback_fn = int (*)(void*, std::function<void(std::string)>);
    const auto create = library.symbol<create_fn>("bambu_network_create_agent");
    const auto destroy = library.symbol<destroy_fn>("bambu_network_destroy_agent");
    const auto set_config = library.symbol<config_fn>("bambu_network_set_config_dir");
    const auto change_user = library.symbol<change_fn>("bambu_network_change_user");
    const auto logout = library.symbol<logout_fn>("bambu_network_user_logout");
    const auto start = library.symbol<start_fn>("bambu_network_start");
    const auto is_logged_in = library.symbol<logged_in_fn>("bambu_network_is_user_login");
    const auto get_name = library.symbol<get_name_fn>("bambu_network_get_user_name");
    const auto login_cmd = library.symbol<login_cmd_fn>("bambu_network_build_login_cmd");
    const auto get_tasks = library.symbol<tasks_fn>("bambu_network_get_user_tasks");
    const auto set_login_callback = library.symbol<login_callback_fn>(
        "bambu_network_set_on_user_login_fn"
    );
    const auto set_printer_callback = library.symbol<printer_callback_fn>(
        "bambu_network_set_on_printer_connected_fn"
    );
    const std::filesystem::path config = argv[3];
    void* agent = create("no-auth-credential-hygiene-probe");
    if (!agent) fail(agent, destroy, "agent creation failed");
    const std::string mode = argv[2];

    if (mode == "persist-failure" || mode == "post-preflight-persist-failure") {
        auto state_path = config;
        if (mode == "persist-failure") {
            state_path = config / "not-a-directory";
            std::filesystem::create_directory(state_path);
            if (set_config(agent, state_path.string()) != 0) {
                fail(agent, destroy, "failed to install deterministic persistence blocker");
            }
            if (std::filesystem::remove_all(state_path) == 0) {
                fail(agent, destroy, "failed to replace persistence directory");
            }
            std::ofstream blocked_path(state_path);
            blocked_path << "block";
            blocked_path.close();
            if (!std::filesystem::is_regular_file(state_path)) {
                fail(agent, destroy, "persistence blocker is not a regular file");
            }
        } else if (set_config(agent, state_path.string()) != 0) {
            fail(agent, destroy, "failed to install post-preflight persistence path");
        }
        std::atomic<int> login_callbacks = 0;
        std::atomic<int> printer_callbacks = 0;
        set_login_callback(agent, [&](int, bool) { ++login_callbacks; });
        set_printer_callback(agent, [&](std::string) { ++printer_callbacks; });
        if (start(agent) != 0) fail(agent, destroy, "plugin start failed");
        std::this_thread::sleep_for(std::chrono::milliseconds(400));
        const auto command = login_cmd(agent);
        if (is_logged_in(agent) || command.find("persist-candidate") != std::string::npos ||
            login_callbacks != 0 || printer_callbacks != 0 ||
            std::filesystem::exists(state_path / "pandar-plugin-login.json")) {
            fail(agent, destroy, "persistence failure left a half-login");
        }
    } else {
        if (mode != "authenticated") {
            const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
            if (!hub_url || hub_url[0] == '\0') {
                fail(agent, destroy, "missing Hub URL for persisted no-auth session");
            }
            std::ofstream login(config / "pandar-plugin-login.json");
            login << "{\"hub_url\":\"" << hub_url
                  << "\",\"token\":\"stale-token\",\"session_kind\":\"no_auth\","
                     "\"profile\":{\"user_id\":\"stale-user\","
                     "\"user_name\":\"Stale User\",\"tenant_id\":\"tenant-1\","
                     "\"tenant_name\":\"Tenant\"}}";
            login.close();
        }
        if (set_config(agent, config.string()) != 0) {
            fail(agent, destroy, "set config dir failed");
        }
        if (mode == "authenticated") {
            const std::string stale =
                R"({"token":"stale-token","user_id":"stale-user","user_name":"Stale User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
            if (change_user(agent, stale) != 0) {
                fail(agent, destroy, "authenticated login setup failed");
            }
            std::string body;
            const int status = get_tasks(agent, BBL::TaskQueryParams{}, &body);
            if (status != -19 || body.find("invalid_auth_token") == std::string::npos) {
                fail(agent, destroy, "authenticated task rejection fell back to no-auth");
            }
        } else if (mode == "concurrent" || mode == "ambiguous") {
            std::atomic<int> ready = 0;
            std::atomic<bool> go = false;
            int status[2] = {-99, -99};
            std::string body[2];
            auto run = [&](int index) {
                ++ready;
                while (!go.load()) std::this_thread::yield();
                status[index] = get_tasks(agent, BBL::TaskQueryParams{}, &body[index]);
            };
            std::thread first(run, 0);
            std::thread second(run, 1);
            while (ready.load() != 2) std::this_thread::yield();
            go = true;
            first.join();
            second.join();
            const bool expected = mode == "concurrent"
                ? status[0] == 0 && status[1] == 0 &&
                    body[0] == R"({"total":0,"hits":[]})" && body[1] == body[0]
                : status[0] == -19 && status[1] == -19 &&
                    body[0].find("ambiguous_no_auth_tenant") != std::string::npos &&
                    body[1] == body[0] &&
                    body[0].find("invalid_auth_token") == std::string::npos;
            if (!expected) {
                fail(
                    agent,
                    destroy,
                    "concurrent task rotation returned an unexpected result: " +
                        std::to_string(status[0]) + "/" + body[0] + "," +
                        std::to_string(status[1]) + "/" + body[1]
                );
            }
        } else {
            int task_status = -99;
            std::string task_body;
            std::thread task([&] {
                task_status = get_tasks(agent, BBL::TaskQueryParams{}, &task_body);
            });
            wait_for(config / "no-auth-post-entered");
            if (mode == "change-race") {
                const std::string replacement =
                    R"({"token":"account-b-token","user_id":"account-b","user_name":"Account B","tenant_id":"tenant-1","tenant_name":"Tenant"})";
                if (change_user(agent, replacement) != 0) {
                    fail(agent, destroy, "concurrent account replacement failed");
                }
            } else if (mode == "logout-race") {
                if (logout(agent, true) != 0) fail(agent, destroy, "concurrent logout failed");
            } else if (mode == "config-race") {
                const auto replacement = config / "replacement-config";
                std::filesystem::create_directory(replacement);
                if (set_config(agent, replacement.string()) != 0) {
                    fail(agent, destroy, "concurrent config change failed");
                }
            } else {
                fail(agent, destroy, "unsupported mode");
            }
            std::ofstream(config / "no-auth-post-release") << "release";
            task.join();
            const auto command = login_cmd(agent);
            const bool state_ok = mode == "change-race"
                ? is_logged_in(agent) && get_name(agent) == "Account B"
                : mode == "logout-race"
                    ? !is_logged_in(agent)
                    : is_logged_in(agent) && get_name(agent) == "Stale User";
            if (task_status != -19 || !state_ok ||
                command.find("race-candidate") != std::string::npos) {
                fail(agent, destroy, "stale no-auth response crossed an account fence");
            }
        }
    }
    destroy(agent);
    std::cout << R"({"ok":true,"mode":")" << mode << R"("})" << '\n';
    return 0;
}
