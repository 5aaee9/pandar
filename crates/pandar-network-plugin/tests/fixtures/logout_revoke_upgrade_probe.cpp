#include <atomic>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <string>
#include <thread>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace BBL {
using OnUserLoginFn = std::function<void(int, bool)>;
using OnHttpErrorFn = std::function<void(unsigned, std::string)>;
}

namespace {
constexpr int kSuccess = 0;
constexpr int kInvalidResult = -19;

struct Library {
#if defined(_WIN32)
    HMODULE handle;
    explicit Library(const char* path) : handle(LoadLibraryA(path)) {}
    ~Library() { if (handle) FreeLibrary(handle); }
    template<class T> T sym(const char* name) const {
        return reinterpret_cast<T>(GetProcAddress(handle, name));
    }
#else
    void* handle;
    explicit Library(const char* path) : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) {}
    ~Library() { if (handle) dlclose(handle); }
    template<class T> T sym(const char* name) const {
        return reinterpret_cast<T>(dlsym(handle, name));
    }
#endif
};

[[noreturn]] void fail(const std::string& message) {
    std::cerr << message << '\n';
    std::exit(2);
}

template<class Fn> Fn required(const Library& library, const char* name) {
    auto function = library.sym<Fn>(name);
    if (!function) fail(std::string("missing symbol ") + name);
    return function;
}
}

int main(int argc, char** argv) {
    if (argc != 4) fail("usage: logout-upgrade-probe <plugin> <mode> <config-dir>");
    const std::string mode = argv[2];
    if (mode != "reentrant-success" && mode != "reentrant-failure" &&
        mode != "reentrant-retained-failure" && mode != "reentrant-retained-disconnect" &&
        mode != "passive-restore" &&
        mode != "late-no-auth-post" && mode != "late-ticket-passive-requested") {
        fail("unsupported probe mode");
    }

    Library library(argv[1]);
    if (!library.handle) fail("failed to load plugin");

    auto create = required<void* (*)(std::string)>(library, "bambu_network_create_agent");
    auto destroy = required<int (*)(void*)>(library, "bambu_network_destroy_agent");
    auto set_config = required<int (*)(void*, std::string)>(library, "bambu_network_set_config_dir");
    auto change_user = required<int (*)(void*, std::string)>(library, "bambu_network_change_user");
    auto set_login = required<int (*)(void*, BBL::OnUserLoginFn)>(
        library, "bambu_network_set_on_user_login_fn"
    );
    auto set_http = required<int (*)(void*, BBL::OnHttpErrorFn)>(
        library, "bambu_network_set_on_http_error_fn"
    );
    auto logout = required<int (*)(void*, bool)>(library, "bambu_network_user_logout");
    auto is_login = required<bool (*)(void*)>(library, "bambu_network_is_user_login");
    auto start = required<int (*)(void*)>(library, "bambu_network_start");
    auto get_token = required<int (*)(void*, std::string, unsigned*, std::string*)>(
        library, "bambu_network_get_my_token"
    );
    auto get_profile = required<int (*)(void*, std::string, unsigned*, std::string*)>(
        library, "bambu_network_get_my_profile"
    );

    void* agent = create("");
    if (!agent || set_config(agent, argv[3]) != kSuccess) fail("agent setup failed");
    const std::string profile =
        R"({"token":"reentrant-upgrade-token","user_id":"upgrade-user","user_name":"Upgrade User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
    const bool retained_failure = mode == "reentrant-retained-failure";
    const bool retained_disconnect = mode == "reentrant-retained-disconnect";
    const bool retained_staging_failure = retained_failure || retained_disconnect;
    const bool passive_restore = mode == "passive-restore";
    const bool reentrant = mode == "reentrant-success" || mode == "reentrant-failure" ||
        retained_staging_failure;
    if ((reentrant || passive_restore) &&
        (change_user(agent, profile) != kSuccess || !is_login(agent))) {
        fail("login setup failed");
    }

    const auto login_file = std::filesystem::path(argv[3]) / "pandar-plugin-login.json";
    const auto pending_file =
        std::filesystem::path(argv[3]) / "pandar-plugin-pending-revocations.json";
    const auto direct_file =
        std::filesystem::path(argv[3]) / "pandar-plugin-direct-revocation.json";
    if (retained_staging_failure && !std::filesystem::create_directory(pending_file)) {
        fail("failed to block pending revocation persistence");
    }
    if (retained_failure && !std::filesystem::create_directory(direct_file)) {
        fail("failed to block direct revocation persistence");
    }
    if (passive_restore) {
        if (!std::filesystem::remove(login_file) ||
            !std::filesystem::create_directory(login_file)) {
            fail("failed to block passive login cleanup");
        }
    }

    int reentrant_status = -999;
    int login_callbacks = 0;
    int logout_callbacks = 0;
    int http_callbacks = 0;
    unsigned http_code = 0;
    std::string http_body;
    bool reentered = false;
    std::vector<std::string> callback_order;
    if (set_login(agent, [&](int, bool login) {
            if (login) {
                ++login_callbacks;
                callback_order.push_back("login");
            } else {
                ++logout_callbacks;
                callback_order.push_back("logout");
            }
            if (reentrant && !login && !reentered) {
                reentered = true;
                reentrant_status = logout(agent, true);
            }
        }) != kSuccess ||
        set_http(agent, [&](unsigned code, std::string body) {
            ++http_callbacks;
            callback_order.push_back("http");
            http_code = code;
            http_body = std::move(body);
        }) != kSuccess) {
        fail("callback setup failed");
    }

    if (mode == "late-no-auth-post") {
        int start_status = -999;
        std::thread starter([&] { start_status = start(agent); });
        const auto entered = std::filesystem::path(argv[3]) / "no-auth-post-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(entered) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::yield();
        }
        if (!std::filesystem::exists(entered)) {
            starter.join();
            fail("no-auth bootstrap did not send its POST");
        }
        const int passive_status = logout(agent, false);
        const int requested_status = logout(agent, true);
        std::ofstream(std::filesystem::path(argv[3]) / "logout-complete") << "complete\n";
        starter.join();
        if (start_status != kSuccess || passive_status != kSuccess ||
            requested_status != kSuccess || is_login(agent) || login_callbacks != 0 ||
            logout_callbacks != 0 || http_callbacks != 0 ||
            std::filesystem::exists(login_file) || std::filesystem::exists(pending_file)) {
            fail("late no-auth response crossed the requested logout fence");
        }
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }
    if (mode == "late-ticket-passive-requested") {
        int exchange_status = -999;
        unsigned exchange_code = 0;
        std::string exchange_body;
        std::thread exchange([&] {
            exchange_status = get_token(
                agent,
                "late-passive-ticket",
                &exchange_code,
                &exchange_body
            );
        });
        const auto entered = std::filesystem::path(argv[3]) / "ticket-post-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(entered) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::yield();
        }
        if (!std::filesystem::exists(entered)) {
            exchange.join();
            fail("ticket exchange did not send its POST");
        }
        const int passive_status = logout(agent, false);
        const int requested_status = logout(agent, true);
        std::ofstream(std::filesystem::path(argv[3]) / "logout-complete") << "complete\n";
        exchange.join();
        if (exchange_status != kInvalidResult || exchange_code != 409 ||
            passive_status != kSuccess || requested_status != kSuccess || is_login(agent) ||
            login_callbacks != 0 || logout_callbacks != 0 || http_callbacks != 0 ||
            std::filesystem::exists(login_file) || std::filesystem::exists(pending_file)) {
            fail("late ticket response crossed the requested logout fence");
        }
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }

    if (passive_restore) {
        const int owner_status = logout(agent, false);
        if (owner_status != kInvalidResult || !is_login(agent) || login_callbacks != 1 ||
            logout_callbacks != 1 || http_callbacks != 0 ||
            callback_order != std::vector<std::string>{"logout", "login"} ||
            std::filesystem::exists(pending_file) || std::filesystem::exists(direct_file)) {
            fail("pure passive cleanup failure did not restore the runtime account");
        }
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }

    const int owner_status = logout(agent, false);
    if (retained_disconnect) {
        if (owner_status != kInvalidResult || reentrant_status != kSuccess || is_login(agent) ||
            login_callbacks != 0 || logout_callbacks != 1 || http_callbacks != 1 ||
            http_code != 0 || http_body != R"({"error":"hub_unavailable"})" ||
            callback_order != std::vector<std::string>{"logout", "http"} ||
            !std::filesystem::is_regular_file(login_file) ||
            !std::filesystem::is_directory(pending_file) ||
            !std::filesystem::is_regular_file(direct_file)) {
            fail("uncertain direct revocation restored a possibly revoked account");
        }
        const int retry_status = logout(agent, true);
        if (retry_status != kSuccess || is_login(agent) || login_callbacks != 0 ||
            logout_callbacks != 1 || http_callbacks != 1 ||
            callback_order != std::vector<std::string>{"logout", "http"} ||
            std::filesystem::exists(login_file) || std::filesystem::exists(direct_file)) {
            fail("uncertain direct revocation was not replayed and completed");
        }
        std::filesystem::remove_all(pending_file);
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }
    if (retained_failure) {
        unsigned profile_code = 0;
        std::string profile_body;
        const int profile_status = get_profile(
            agent,
            "reentrant-upgrade-token",
            &profile_code,
            &profile_body
        );
        if (owner_status != kInvalidResult || reentrant_status != kSuccess ||
            !is_login(agent) || login_callbacks != 1 || logout_callbacks != 1 ||
            http_callbacks != 1 || http_code != 0 ||
            http_body != R"({"error":"account_state_unavailable"})" ||
            callback_order != std::vector<std::string>{"logout", "login", "http"} ||
            profile_status != kSuccess || profile_code != 200 ||
            profile_body.find("upgrade-user") == std::string::npos ||
            !std::filesystem::is_regular_file(login_file) ||
            !std::filesystem::is_directory(pending_file) ||
            !std::filesystem::is_directory(direct_file)) {
            fail("direct intent persistence failure did not restore the retained account");
        }
        std::ofstream(std::filesystem::path(argv[3]) / "retained-restore-complete")
            << "complete\n";
        const auto release =
            std::filesystem::path(argv[3]) / "release-retained-retry";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(release) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::yield();
        }
        if (!std::filesystem::exists(release)) fail("retained retry was not released");
        std::filesystem::remove_all(pending_file);
        std::filesystem::remove_all(direct_file);
        const int retry_status = logout(agent, true);
        if (retry_status != kSuccess || is_login(agent) || login_callbacks != 1 ||
            logout_callbacks != 2 || http_callbacks != 1 ||
            callback_order !=
                std::vector<std::string>{"logout", "login", "http", "logout"} ||
            std::filesystem::exists(login_file) || std::filesystem::exists(pending_file) ||
            std::filesystem::exists(direct_file)) {
            fail("restored account could not complete a later requested logout");
        }
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }
    const bool failure = mode == "reentrant-failure";
    const bool callbacks_valid = failure
        ? http_callbacks == 1 && http_code == 503 &&
            http_body == R"({"error":"invalid_response"})"
        : http_callbacks == 0;
    const bool pending_valid = failure
        ? std::filesystem::exists(pending_file)
        : !std::filesystem::exists(pending_file);
    if (owner_status != (failure ? kInvalidResult : kSuccess) ||
        reentrant_status != kSuccess || is_login(agent) || login_callbacks != 0 ||
        logout_callbacks != 1 ||
        !callbacks_valid || !pending_valid || std::filesystem::exists(login_file)) {
        fail("reentrant requested logout was not consumed by the passive owner");
    }
    if (destroy(agent) != kSuccess) fail("destroy failed");
    std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
}
