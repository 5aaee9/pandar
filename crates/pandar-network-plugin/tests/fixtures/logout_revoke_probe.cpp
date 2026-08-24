#include <atomic>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <iterator>
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
using OnLocalConnectedFn = std::function<void(int, std::string, std::string)>;
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

std::string read_file(const std::filesystem::path& path) {
    std::ifstream input(path, std::ios::binary);
    return {
        std::istreambuf_iterator<char>(input),
        std::istreambuf_iterator<char>()
    };
}
}

int main(int argc, char** argv) {
    if (argc != 4) fail("usage: logout-revoke-probe <plugin> <mode> <config-dir>");
    const std::string mode = argv[2];
    if (mode != "success" && mode != "local" && mode != "empty" &&
        mode != "stale-observation" && mode != "failure" &&
        mode != "disconnect" && mode != "timeout" && mode != "timeout-relogin" &&
        mode != "repeat" && mode != "failure-retry" && mode != "failure-restart" &&
        mode != "local-failure" && mode != "bootstrap-logout-race" &&
        mode != "ticket-logout-race" && mode != "ticket-passive-control" &&
        mode != "stage-failure-delete-success" &&
        mode != "stage-failure-delete-delayed-success" &&
        mode != "stage-failure-delete-failure" &&
        mode != "stage-failure-delete-relogin-success" &&
        mode != "stage-failure-delete-relogin-failure" &&
        mode != "stage-failure-delete-unauthorized" &&
        mode != "stage-failure-delete-gone") {
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
    auto set_local = required<int (*)(void*, BBL::OnLocalConnectedFn)>(
        library, "bambu_network_set_on_local_connect_fn"
    );
    auto get_print_info = required<int (*)(void*, unsigned*, std::string*)>(
        library, "bambu_network_get_user_print_info"
    );
    auto get_token = required<int (*)(void*, std::string, unsigned*, std::string*)>(
        library, "bambu_network_get_my_token"
    );
    auto connect_printer = required<int (*)(
        void*, std::string, std::string, std::string, std::string, bool
    )>(library, "bambu_network_connect_printer");
    auto logout = required<int (*)(void*, bool)>(library, "bambu_network_user_logout");
    auto is_login = required<bool (*)(void*)>(library, "bambu_network_is_user_login");
    auto get_user_id = required<std::string (*)(void*)>(library, "bambu_network_get_user_id");
    auto get_user_name = required<std::string (*)(void*)>(
        library, "bambu_network_get_user_name"
    );
    auto start = required<int (*)(void*)>(library, "bambu_network_start");

    void* agent = create("");
    if (!agent || set_config(agent, argv[3]) != kSuccess) fail("agent setup failed");
    const std::string profile =
        R"({"token":"logout-secret-token","user_id":"logout-user","user_name":"Logout User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
    const auto login_file = std::filesystem::path(argv[3]) / "pandar-plugin-login.json";
    const auto pending_file =
        std::filesystem::path(argv[3]) / "pandar-plugin-pending-revocations.json";
    const auto direct_file =
        std::filesystem::path(argv[3]) / "pandar-plugin-direct-revocation.json";
    const bool starts_empty = mode == "empty" || mode == "stale-observation" ||
        mode == "bootstrap-logout-race" || mode == "ticket-logout-race" ||
        mode == "ticket-passive-control";
    if (!starts_empty) {
        if (change_user(agent, profile) != kSuccess || !is_login(agent)) {
            fail("login setup failed");
        }
        if (!std::filesystem::exists(login_file)) fail("login was not persisted");
    }
    const std::string original_login = starts_empty ? "" : read_file(login_file);

    std::vector<std::string> events;
    std::atomic<bool> printer_callback = false;
    std::atomic<bool> logout_callback = false;
    unsigned http_code = 0;
    std::string http_body;
    if (set_login(agent, [&](int, bool login) {
            events.push_back(login ? "login" : "logout");
            if (!login) logout_callback.store(true, std::memory_order_release);
        }) != kSuccess ||
        set_http(agent, [&](unsigned code, std::string body) {
            http_code = code;
            http_body = std::move(body);
            events.push_back("http");
        }) != kSuccess ||
        set_local(agent, [&](int state, std::string, std::string) {
            if (state == 2) {
                events.push_back("printer");
                printer_callback.store(true, std::memory_order_release);
            }
        }) != kSuccess) {
        fail("callback setup failed");
    }

    if (mode == "failure" || mode == "failure-retry" || mode == "failure-restart" ||
        mode == "disconnect" || mode == "timeout" || mode == "timeout-relogin") {
        unsigned code = 0;
        std::string body;
        if (get_print_info(agent, &code, &body) != kSuccess || code != 200 ||
            connect_printer(agent, "logout-printer", "127.0.0.1", "user", "pass", false)
                != kSuccess) {
            fail("printer transition setup failed");
        }
    }

    if (mode == "local-failure") {
        std::filesystem::remove(login_file);
        if (!std::filesystem::create_directory(login_file)) {
            fail("failed to arrange the local clear failure");
        }
    }

    if (mode == "stage-failure-delete-success" ||
        mode == "stage-failure-delete-delayed-success" ||
        mode == "stage-failure-delete-failure" ||
        mode == "stage-failure-delete-relogin-success" ||
        mode == "stage-failure-delete-relogin-failure" ||
        mode == "stage-failure-delete-unauthorized" ||
        mode == "stage-failure-delete-gone") {
        if (!std::filesystem::create_directory(pending_file)) {
            fail("failed to arrange the pending-revocation staging failure");
        }
    }

    if (mode == "bootstrap-logout-race") {
        const auto* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
        if (!hub_url) fail("bootstrap race has no Hub URL");
        std::ofstream pending(pending_file);
        pending << "[{\"hub_url\":\"" << hub_url
                << "\",\"token\":\"pending-bootstrap-token\"}]";
        pending.close();

        int start_status = -999;
        std::thread bootstrap([&] { start_status = start(agent); });
        const auto entered = std::filesystem::path(argv[3]) / "bootstrap-delete-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(entered) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (!std::filesystem::exists(entered)) {
            bootstrap.join();
            fail("bootstrap did not enter the blocked pending DELETE");
        }
        const auto logout_status = logout(agent, true);
        bootstrap.join();
        if (start_status != kSuccess || logout_status != kSuccess || is_login(agent) ||
            std::filesystem::exists(login_file) || std::filesystem::exists(pending_file) ||
            !events.empty()) {
            fail("requested logout did not fence the blocked startup bootstrap");
        }
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }

    if (mode == "ticket-logout-race" || mode == "ticket-passive-control") {
        int exchange_status = -999;
        unsigned exchange_code = 0;
        std::string exchange_body;
        std::thread exchange([&] {
            exchange_status = get_token(
                agent,
                mode == "ticket-logout-race" ? "requested-race-ticket" : "passive-ticket",
                &exchange_code,
                &exchange_body
            );
        });
        const auto entered = std::filesystem::path(argv[3]) / "ticket-exchange-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(entered) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (!std::filesystem::exists(entered)) {
            exchange.join();
            fail("ticket exchange did not enter the blocked response");
        }
        const auto logout_status = logout(agent, mode == "ticket-logout-race");
        std::ofstream(
            std::filesystem::path(argv[3]) / "ticket-logout-complete"
        ) << "complete\n";
        exchange.join();

        const bool requested = mode == "ticket-logout-race";
        const bool valid = requested
            ? logout_status == kSuccess && exchange_status == kInvalidResult &&
                exchange_code == 409 && !is_login(agent) &&
                !std::filesystem::exists(login_file) &&
                !std::filesystem::exists(pending_file) &&
                events.empty()
            : logout_status == kSuccess && exchange_status == kSuccess &&
                exchange_code == 200 && is_login(agent) &&
                std::filesystem::exists(login_file) &&
                events == std::vector<std::string>{"login"};
        if (!valid) fail("tokenless ticket exchange observed the wrong logout fence");
        if (destroy(agent) != kSuccess) fail("destroy failed");
        std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
        return 0;
    }

    int status = kSuccess;
    if (mode == "stale-observation") {
        if (is_login(agent)) fail("stale observation setup was unexpectedly logged in");
        std::thread commit([&] {
            if (change_user(agent, profile) != kSuccess) fail("background login commit failed");
        });
        commit.join();
        status = logout(agent, false);
    } else if (mode == "timeout" || mode == "timeout-relogin") {
        std::atomic<bool> logout_done = false;
        std::thread logout_thread([&] {
            status = logout(agent, true);
            logout_done.store(true, std::memory_order_release);
        });
        const auto local_deadline = std::chrono::steady_clock::now() + std::chrono::seconds(1);
        while (!logout_callback.load(std::memory_order_acquire) &&
               std::chrono::steady_clock::now() < local_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
        if (!logout_callback.load(std::memory_order_acquire) ||
            !printer_callback.load(std::memory_order_acquire)) {
            fail("unresponsive Hub delayed local logout callbacks");
        }
        if (is_login(agent) || std::filesystem::exists(login_file)) {
            fail("unresponsive Hub delayed local logout state clearing");
        }
        if (mode == "timeout-relogin") {
            const std::string replacement_profile =
                R"({"token":"replacement-token","user_id":"replacement-user","user_name":"Replacement User","tenant_id":"tenant-2","tenant_name":"Replacement Tenant"})";
            if (change_user(agent, replacement_profile) != kSuccess || !is_login(agent) ||
                !std::filesystem::exists(login_file)) {
                fail("replacement account did not commit during the stale logout request");
            }
        }
        const auto operation_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(4);
        while (!logout_done.load(std::memory_order_acquire) &&
               std::chrono::steady_clock::now() < operation_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
        if (!logout_done.load(std::memory_order_acquire)) {
            fail("unresponsive Hub blocked logout beyond its finite bound");
        }
        logout_thread.join();
        if (mode == "timeout") {
            const auto retry_checked =
                std::filesystem::path(argv[3]) / "timeout-no-immediate-retry";
            const auto retry_deadline =
                std::chrono::steady_clock::now() + std::chrono::seconds(3);
            while (!std::filesystem::exists(retry_checked) &&
                   std::chrono::steady_clock::now() < retry_deadline) {
                std::this_thread::sleep_for(std::chrono::milliseconds(10));
            }
            if (!std::filesystem::exists(retry_checked)) {
                fail("mock Hub did not complete the immediate retry check");
            }
        }
    } else if (mode == "stage-failure-delete-delayed-success" ||
               mode == "stage-failure-delete-relogin-success" ||
               mode == "stage-failure-delete-relogin-failure") {
        std::atomic<bool> logout_done = false;
        std::thread logout_thread([&] {
            status = logout(agent, true);
            logout_done.store(true, std::memory_order_release);
        });
        const auto entered = std::filesystem::path(argv[3]) / "unstaged-delete-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (!std::filesystem::exists(entered) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (!std::filesystem::exists(entered) || logout_done.load(std::memory_order_acquire) ||
            !is_login(agent) || read_file(login_file) != original_login ||
            logout_callback.load(std::memory_order_acquire)) {
            fail("unstaged DELETE changed local state before its response");
        }
        const bool replacement = mode == "stage-failure-delete-relogin-success" ||
            mode == "stage-failure-delete-relogin-failure";
        if (replacement) {
            std::filesystem::remove_all(pending_file);
            const std::string replacement_profile =
                R"({"token":"replacement-token","user_id":"replacement-user","user_name":"Replacement User","tenant_id":"tenant-2","tenant_name":"Replacement Tenant"})";
            if (change_user(agent, replacement_profile) != kSuccess ||
                get_user_id(agent) != "replacement-user" ||
                get_user_name(agent) != "Replacement User") {
                fail("replacement account did not commit during unstaged DELETE");
            }
        }
        std::ofstream(
            std::filesystem::path(argv[3]) / "release-unstaged-delete"
        ) << "release\n";
        logout_thread.join();
        if (replacement &&
            (!is_login(agent) || get_user_id(agent) != "replacement-user" ||
             get_user_name(agent) != "Replacement User" ||
             !std::filesystem::exists(login_file))) {
            fail("unstaged DELETE changed the replacement account");
        }
    } else {
        status = logout(agent, mode != "local");
    }
    int repeated_status = kSuccess;
    if (mode == "repeat") repeated_status = logout(agent, true);
    bool pending_after_failure = false;
    if (mode == "failure-retry") {
        pending_after_failure = std::filesystem::exists(pending_file);
        repeated_status = logout(agent, true);
    }
    if (mode == "failure-restart") {
        pending_after_failure = std::filesystem::exists(pending_file);
        if (destroy(agent) != kSuccess) fail("destroy before pending retry failed");
        agent = create("");
        if (!agent || set_config(agent, argv[3]) != kSuccess || start(agent) != kSuccess) {
            fail("restart did not recover the pending revoke");
        }
    }
    if (mode == "stage-failure-delete-failure") {
        if (status != kInvalidResult || is_login(agent) ||
            read_file(login_file) != original_login ||
            !std::filesystem::is_directory(pending_file) ||
            !std::filesystem::is_regular_file(direct_file) ||
            events != std::vector<std::string>{"logout", "http"}) {
            fail("ambiguous fallback DELETE did not retain its durable intent");
        }
        repeated_status = logout(agent, true);
    }
    const bool expected_failure =
        mode == "failure" || mode == "disconnect" || mode == "timeout" ||
        mode == "timeout-relogin" || mode == "failure-retry" ||
        mode == "failure-restart" || mode == "local-failure" ||
        mode == "stage-failure-delete-failure" ||
        mode == "stage-failure-delete-relogin-failure";
    const int expected_status = expected_failure ? kInvalidResult : kSuccess;
    if (status != expected_status) fail("logout returned the wrong ABI status");
    if (repeated_status != kSuccess) fail("repeated logout was not idempotent");
    if ((mode == "failure-retry" || mode == "failure-restart") &&
        (!pending_after_failure || std::filesystem::exists(pending_file))) {
        fail("failed revoke was not durably retried and cleared");
    }
    if (mode == "timeout-relogin" || mode == "stale-observation" ||
        mode == "failure-restart" ||
        mode == "stage-failure-delete-relogin-success" ||
        mode == "stage-failure-delete-relogin-failure") {
        if (!is_login(agent) || !std::filesystem::exists(login_file)) {
            fail("logout failure did not preserve the retryable account");
        }
    } else if (is_login(agent)) {
        fail("logout left the token in memory");
    }
    if (mode == "local-failure") {
        if (!std::filesystem::is_directory(login_file)) {
            fail("local clear failure did not retain its failure evidence");
        }
        if (!std::filesystem::is_regular_file(pending_file)) {
            fail("local clear failure did not retain its revocation tombstone");
        }
    } else if (mode == "local" && std::filesystem::exists(pending_file)) {
        fail("local logout left its provisional revocation tombstone");
    } else if (mode != "timeout-relogin" && mode != "stale-observation" &&
               mode != "failure-restart" &&
               mode != "stage-failure-delete-relogin-success" &&
               mode != "stage-failure-delete-relogin-failure" &&
               std::filesystem::exists(login_file)) {
        fail("logout left the persisted login");
    }
    const auto expected_events = mode == "timeout-relogin"
        ? std::vector<std::string>{"printer", "logout", "login"}
        : mode == "stage-failure-delete-relogin-success" ||
              mode == "stage-failure-delete-relogin-failure"
            ? std::vector<std::string>{"login"}
        : mode == "stale-observation"
            ? std::vector<std::string>{"login"}
        : mode == "failure" || mode == "disconnect" || mode == "timeout" ||
              mode == "failure-retry" || mode == "failure-restart"
        ? std::vector<std::string>{"printer", "logout", "http"}
        : mode == "stage-failure-delete-failure"
            ? std::vector<std::string>{"logout", "http"}
        : mode == "repeat"
            ? std::vector<std::string>{"logout"}
        : mode == "empty"
            ? std::vector<std::string>{}
        : std::vector<std::string>{"logout"};
    const auto order_deadline =
        std::chrono::steady_clock::now() + std::chrono::seconds(5);
    while (events.size() < expected_events.size() &&
           std::chrono::steady_clock::now() < order_deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }
    if (events != expected_events) {
        fail("logout callbacks were missing or out of order; got=" +
            std::to_string(events.size()) + ":" +
            [&]() {
                std::string joined;
                for (const auto& e : events) joined += e + ",";
                return joined;
            }());
    }
    if (mode == "failure" &&
        (http_code != 500 || http_body != R"({"error":"invalid_response"})")) {
        fail("remote failure was not delivered through the redacted HTTP callback");
    }
    if (mode == "disconnect" &&
        (http_code != 0 || http_body != R"({"error":"hub_unavailable"})")) {
        fail("disconnect was not delivered through the redacted HTTP callback");
    }
    if (mode == "timeout" &&
        (http_code != 0 || http_body != R"({"error":"hub_unavailable"})")) {
        fail("timeout was not delivered through the redacted HTTP callback");
    }
    if (mode == "stage-failure-delete-failure" &&
        (http_code != 503 || http_body != R"({"error":"invalid_response"})")) {
        fail("failed fallback DELETE was not delivered through the redacted HTTP callback");
    }
    if (destroy(agent) != kSuccess) fail("destroy failed");

    std::cout << "{\"ok\":true,\"mode\":\"" << mode << "\"}" << '\n';
}
