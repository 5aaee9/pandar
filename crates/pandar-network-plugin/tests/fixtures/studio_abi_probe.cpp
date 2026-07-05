#include <cstdint>
#include <cstdlib>
#include <atomic>
#include <chrono>
#include <functional>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <thread>
#include <vector>

#if defined(_WIN32)
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#else
#include <dlfcn.h>
#include <sys/socket.h>
#include <unistd.h>
#include <netdb.h>
#endif

namespace BBL {

using OnUpdateStatusFn = std::function<void(int, int, std::string)>;
using WasCancelledFn = std::function<bool()>;
using OnWaitFn = std::function<bool(int, std::string)>;
using OnPrinterConnectedFn = std::function<void(std::string)>;
using OnLocalConnectedFn = std::function<void(int, std::string, std::string)>;
using OnServerConnectedFn = std::function<void(int, int)>;
using OnMessageFn = std::function<void(std::string, std::string)>;

struct PrintParams {
    std::string dev_id;
    std::string task_name;
    std::string project_name;
    std::string preset_name;
    std::string filename;
    std::string config_filename;
    int plate_index = 0;
    std::string ftp_folder;
    std::string ftp_file;
    std::string ftp_file_md5;
    std::string nozzle_mapping;
    std::string ams_mapping;
    std::string ams_mapping2;
    std::string ams_mapping_info;
    std::string nozzles_info;
    std::string connection_type;
    std::string comments;
    int origin_profile_id = 0;
    int stl_design_id = 0;
    std::string origin_model_id;
    std::string print_type;
    std::string dst_file;
    std::string dev_name;
    std::string dev_ip;
    bool use_ssl_for_ftp = false;
    bool use_ssl_for_mqtt = false;
    std::string username;
    std::string password;
    bool task_bed_leveling = false;
    bool task_flow_cali = false;
    bool task_vibration_cali = false;
    bool task_layer_inspect = false;
    bool task_record_timelapse = false;
    bool task_timelapse_use_internal = false;
    bool task_use_ams = false;
    std::string task_bed_type;
    std::string extra_options;
    int auto_bed_leveling = 0;
    int auto_flow_cali = 0;
    int auto_offset_cali = 0;
    int extruder_cali_manual_mode = -1;
    bool task_ext_change_assist = false;
    bool try_emmc_print = false;
    std::string svc_context;
};

struct TaskQueryParams {
    std::string dev_id;
    int status = 0;
    int offset = 0;
    int limit = 20;
};

} // namespace BBL

extern "C" {

struct ft_job_result {
    int ec;
    int resp_ec;
    const char* json;
    const void* bin;
    uint32_t bin_size;
};

struct FT_TunnelHandle;
struct FT_JobHandle;

using ft_tunnel_connect_cb = void (*)(void* user, int ok, int err, const char* msg);
using ft_job_result_cb = void (*)(void* user, ft_job_result result);

} // extern "C"

namespace {

constexpr int kFtOk = 0;
constexpr int kFtEio = -3;

std::string escape_json(const std::string& value) {
    std::string out;
    out.reserve(value.size() + 2);
    out.push_back('"');
    for (char c : value) {
        switch (c) {
            case '\\': out += "\\\\"; break;
            case '"': out += "\\\""; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default: out.push_back(c); break;
        }
    }
    out.push_back('"');
    return out;
}

bool contains(const std::string& haystack, const std::string& needle) {
    return haystack.find(needle) != std::string::npos;
}

struct ParsedUrl {
    std::string host;
    std::string port;
};

ParsedUrl parse_http_loopback_url(const std::string& url) {
    const std::string prefix = "http://";
    if (url.rfind(prefix, 0) != 0) {
        std::cerr << "expected http URL: " << url << "\n";
        std::exit(2);
    }
    auto authority = url.substr(prefix.size());
    auto slash = authority.find('/');
    if (slash != std::string::npos) authority.resize(slash);
    auto colon = authority.rfind(':');
    if (colon == std::string::npos) {
        std::cerr << "expected explicit port in URL: " << url << "\n";
        std::exit(2);
    }
    return {authority.substr(0, colon), authority.substr(colon + 1)};
}

std::string http_request(const std::string& base_url, const std::string& request) {
    auto parsed = parse_http_loopback_url(base_url);
#if defined(_WIN32)
    WSADATA data;
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0) {
        std::cerr << "WSAStartup failed\n";
        std::exit(2);
    }
#endif
    addrinfo hints{};
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    addrinfo* addrs = nullptr;
    if (getaddrinfo(parsed.host.c_str(), parsed.port.c_str(), &hints, &addrs) != 0 || !addrs) {
        std::cerr << "getaddrinfo failed for local webserver\n";
        std::exit(2);
    }
    int fd = -1;
    for (auto* addr = addrs; addr; addr = addr->ai_next) {
        fd = static_cast<int>(socket(addr->ai_family, addr->ai_socktype, addr->ai_protocol));
        if (fd < 0) continue;
        if (connect(fd, addr->ai_addr, addr->ai_addrlen) == 0) break;
#if defined(_WIN32)
        closesocket(fd);
#else
        close(fd);
#endif
        fd = -1;
    }
    freeaddrinfo(addrs);
    if (fd < 0) {
        std::cerr << "connect local webserver failed\n";
        std::exit(2);
    }
    const char* cursor = request.data();
    std::size_t remaining = request.size();
    while (remaining > 0) {
#if defined(_WIN32)
        int sent = send(fd, cursor, static_cast<int>(remaining), 0);
#else
        auto sent = send(fd, cursor, remaining, 0);
#endif
        if (sent <= 0) {
            std::cerr << "send local webserver request failed\n";
            std::exit(2);
        }
        cursor += sent;
        remaining -= static_cast<std::size_t>(sent);
    }
    std::string response;
    char buffer[1024];
    for (;;) {
#if defined(_WIN32)
        int read = recv(fd, buffer, sizeof(buffer), 0);
#else
        auto read = recv(fd, buffer, sizeof(buffer), 0);
#endif
        if (read <= 0) break;
        response.append(buffer, buffer + read);
    }
#if defined(_WIN32)
    closesocket(fd);
    WSACleanup();
#else
    close(fd);
#endif
    return response;
}

std::string http_body(const std::string& response) {
    auto marker = response.find("\r\n\r\n");
    return marker == std::string::npos ? std::string{} : response.substr(marker + 4);
}

std::string json_field(const std::string& body, const std::string& name) {
    const std::string key = "\"" + name + "\":\"";
    auto start = body.find(key);
    if (start == std::string::npos) return {};
    start += key.size();
    auto end = body.find('"', start);
    return end == std::string::npos ? std::string{} : body.substr(start, end - start);
}

void switch_local_hub_target(const std::string& base_url, const std::string& hub_url) {
    auto config_response = http_request(
        base_url,
        "GET /config HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    auto config_body = http_body(config_response);
    auto web_url = json_field(config_body, "webUrl");
    auto nonce = json_field(config_body, "configNonce");
    if (web_url.empty() || nonce.empty()) {
        std::cerr << "local config response lacked webUrl or configNonce: " << config_body << "\n";
        std::exit(2);
    }
    auto post_body = std::string("{\"webUrl\":\"") + web_url + "\",\"hubUrl\":\"" + hub_url + "\",\"configNonce\":\"" + nonce + "\"}";
    auto request = std::string("POST /config HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: ") +
        std::to_string(post_body.size()) + "\r\nConnection: close\r\n\r\n" + post_body;
    auto response = http_request(base_url, request);
    if (!contains(response, "HTTP/1.1 200 OK")) {
        std::cerr << "switch local hub target failed: " << response << "\n";
        std::exit(2);
    }
}

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

    bool ok() const { return handle != nullptr; }

    template <class T>
    T sym(const char* name) const {
#if defined(_WIN32)
        auto* raw = handle ? reinterpret_cast<void*>(GetProcAddress(handle, name)) : nullptr;
#else
        auto* raw = handle ? dlsym(handle, name) : nullptr;
#endif
        if (!raw) {
            std::cerr << "missing symbol: " << name << "\n";
            std::exit(3);
        }
        return reinterpret_cast<T>(raw);
    }
};

struct ProbeResult {
    bool ok = true;
    std::string host;
    std::string login_command;
    std::string login_info;
    std::string logout_command;
    int printer_rc = 0;
    int tasks_rc = 0;
    int print_rc = 0;
    int direct_connect_rc = 0;
    int direct_message_rc = 0;
    bool direct_connect_callback = false;
    bool local_connect_callback = false;
    bool message_callback = false;
    bool version_callback = false;
    int selected_machine_messages = 0;
    int subscribe_messages = 0;
    int heartbeat_messages = 0;
    int connect_messages = 0;
    std::string camera_url;
    int ft_abi_version = 0;
    int ft_start_connect_rc = 0;
    int ft_sync_rc = 0;
    int ft_start_job_rc = 0;
    int ft_job_result_ec = 0;
    int ft_cancel_rc = 0;
    std::string update_body;
    std::string sdcard_update_body;
    bool restored_login = false;
};

[[noreturn]] void fail(void* agent, int (*destroy_agent)(void*), const std::string& message) {
    if (agent && destroy_agent) destroy_agent(agent);
    std::cerr << message << "\n";
    std::exit(2);
}

void assert_redacted_stable_error(
    void* agent,
    int (*destroy_agent)(void*),
    const std::string& body,
    const std::string& stable_error,
    const std::vector<std::string>& forbidden
) {
    if (!contains(body, stable_error)) {
        fail(agent, destroy_agent, "ABI body did not contain stable error: " + stable_error);
    }
    for (const auto& value : forbidden) {
        if (contains(body, value)) {
            fail(agent, destroy_agent, "ABI body leaked forbidden value: " + value);
        }
    }
}

void print_json(const ProbeResult& result) {
    std::cout
        << "{"
        << "\"ok\":" << (result.ok ? "true" : "false")
        << ",\"host\":" << escape_json(result.host)
        << ",\"login_command\":" << escape_json(result.login_command)
        << ",\"login_info\":" << escape_json(result.login_info)
        << ",\"logout_command\":" << escape_json(result.logout_command)
        << ",\"printer_rc\":" << result.printer_rc
        << ",\"tasks_rc\":" << result.tasks_rc
        << ",\"print_rc\":" << result.print_rc
        << ",\"direct_connect_rc\":" << result.direct_connect_rc
        << ",\"direct_message_rc\":" << result.direct_message_rc
        << ",\"direct_connect_callback\":" << (result.direct_connect_callback ? "true" : "false")
        << ",\"local_connect_callback\":" << (result.local_connect_callback ? "true" : "false")
        << ",\"message_callback\":" << (result.message_callback ? "true" : "false")
        << ",\"version_callback\":" << (result.version_callback ? "true" : "false")
        << ",\"selected_machine_messages\":" << result.selected_machine_messages
        << ",\"subscribe_messages\":" << result.subscribe_messages
        << ",\"heartbeat_messages\":" << result.heartbeat_messages
        << ",\"connect_messages\":" << result.connect_messages
        << ",\"camera_url\":" << escape_json(result.camera_url)
        << ",\"ft_abi_version\":" << result.ft_abi_version
        << ",\"ft_start_connect_rc\":" << result.ft_start_connect_rc
        << ",\"ft_sync_rc\":" << result.ft_sync_rc
        << ",\"ft_start_job_rc\":" << result.ft_start_job_rc
        << ",\"ft_job_result_ec\":" << result.ft_job_result_ec
        << ",\"ft_cancel_rc\":" << result.ft_cancel_rc
        << ",\"update_body\":" << escape_json(result.update_body)
        << ",\"sdcard_update_body\":" << escape_json(result.sdcard_update_body)
        << ",\"restored_login\":" << (result.restored_login ? "true" : "false")
        << "}\n";
}

void ft_connect_cb(void* user, int, int, const char* msg) {
    auto* out = reinterpret_cast<std::string*>(user);
    *out = msg ? msg : "";
}

void ft_result_cb(void* user, ft_job_result result) {
    auto* out = reinterpret_cast<int*>(user);
    *out = result.ec;
}

} // namespace

int main(int argc, char** argv) {
    if (argc < 3) {
        std::cerr << "usage: studio_abi_probe <plugin-library> <artifact-path> [success|failure]\n";
        return 2;
    }
    const std::string mode = argc >= 4 ? argv[3] : "success";
    const bool failure_mode = mode == "failure";
    if (!failure_mode && mode != "success") {
        std::cerr << "mode must be success or failure\n";
        return 2;
    }

    Library lib(argv[1]);
    if (!lib.ok()) {
        std::cerr << "failed to load plugin library\n";
        return 3;
    }

    using create_agent_fn = void* (*)(std::string);
    using destroy_agent_fn = int (*)(void*);
    using string_agent_fn = std::string (*)(void*);
    using start_fn = int (*)(void*);
    using set_config_dir_fn = int (*)(void*, std::string);
    using token_fn = int (*)(void*, std::string, unsigned int*, std::string*);
    using change_user_fn = int (*)(void*, std::string);
    using is_user_login_fn = bool (*)(void*);
    using print_info_fn = int (*)(void*, unsigned int*, std::string*);
    using tasks_fn = int (*)(void*, BBL::TaskQueryParams, std::string*);
    using start_print_fn = int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn);
    using start_sdcard_fn = int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn);
    using connect_printer_fn = int (*)(void*, std::string, std::string, std::string, std::string, bool);
    using send_cloud_fn = int (*)(void*, std::string, std::string, int, int);
    using send_printer_fn = int (*)(void*, std::string, std::string, int, int);
    using selected_machine_fn = int (*)(void*, std::string);
    using start_subscribe_fn = int (*)(void*, std::string);
    using add_subscribe_fn = int (*)(void*, std::vector<std::string>);
    using set_printer_connected_fn = int (*)(void*, BBL::OnPrinterConnectedFn);
    using set_server_connected_fn = int (*)(void*, BBL::OnServerConnectedFn);
    using set_local_connect_fn = int (*)(void*, BBL::OnLocalConnectedFn);
    using set_message_fn = int (*)(void*, BBL::OnMessageFn);
    using connect_server_fn = int (*)(void*);
    using update_cert_fn = int (*)(void*);
    using camera_url_fn = int (*)(void*, std::string, std::function<void(std::string)>);
    using logout_fn = int (*)(void*, bool);

    auto create_agent = lib.sym<create_agent_fn>("bambu_network_create_agent");
    auto destroy_agent = lib.sym<destroy_agent_fn>("bambu_network_destroy_agent");
    auto start = lib.sym<start_fn>("bambu_network_start");
    auto set_config_dir = lib.sym<set_config_dir_fn>("bambu_network_set_config_dir");
    auto get_host = lib.sym<string_agent_fn>("bambu_network_get_bambulab_host");
    auto get_token = lib.sym<token_fn>("bambu_network_get_my_token");
    auto get_profile = lib.sym<token_fn>("bambu_network_get_my_profile");
    auto change_user = lib.sym<change_user_fn>("bambu_network_change_user");
    auto is_user_login = lib.sym<is_user_login_fn>("bambu_network_is_user_login");
    auto build_login_cmd = lib.sym<string_agent_fn>("bambu_network_build_login_cmd");
    auto build_login_info = lib.sym<string_agent_fn>("bambu_network_build_login_info");
    auto get_print_info = lib.sym<print_info_fn>("bambu_network_get_user_print_info");
    auto get_selected_machine = lib.sym<string_agent_fn>("bambu_network_get_user_selected_machine");
    auto get_tasks = lib.sym<tasks_fn>("bambu_network_get_user_tasks");
    auto start_print = lib.sym<start_print_fn>("bambu_network_start_print");
    auto start_sdcard_print = lib.sym<start_sdcard_fn>("bambu_network_start_send_gcode_to_sdcard");
    auto connect_printer = lib.sym<connect_printer_fn>("bambu_network_connect_printer");
    auto send_cloud = lib.sym<send_cloud_fn>("bambu_network_send_message");
    auto send_printer = lib.sym<send_printer_fn>("bambu_network_send_message_to_printer");
    auto set_selected_machine = lib.sym<selected_machine_fn>("bambu_network_set_user_selected_machine");
    auto start_subscribe = lib.sym<start_subscribe_fn>("bambu_network_start_subscribe");
    auto add_subscribe = lib.sym<add_subscribe_fn>("bambu_network_add_subscribe");
    auto set_printer_connected = lib.sym<set_printer_connected_fn>("bambu_network_set_on_printer_connected_fn");
    auto set_server_connected = lib.sym<set_server_connected_fn>("bambu_network_set_on_server_connected_fn");
    auto set_local_connect = lib.sym<set_local_connect_fn>("bambu_network_set_on_local_connect_fn");
    auto set_message = lib.sym<set_message_fn>("bambu_network_set_on_message_fn");
    auto connect_server = lib.sym<connect_server_fn>("bambu_network_connect_server");
    auto update_cert = lib.sym<update_cert_fn>("bambu_network_update_cert");
    auto get_camera_url = lib.sym<camera_url_fn>("bambu_network_get_camera_url");
    auto user_logout = lib.sym<logout_fn>("bambu_network_user_logout");
    auto build_logout_cmd = lib.sym<string_agent_fn>("bambu_network_build_logout_cmd");

    auto ft_abi_version = lib.sym<int (*)()>("ft_abi_version");
    auto ft_tunnel_create = lib.sym<int (*)(const char*, FT_TunnelHandle**)>("ft_tunnel_create");
    auto ft_tunnel_start_connect = lib.sym<int (*)(FT_TunnelHandle*, ft_tunnel_connect_cb, void*)>("ft_tunnel_start_connect");
    auto ft_tunnel_sync_connect = lib.sym<int (*)(FT_TunnelHandle*)>("ft_tunnel_sync_connect");
    auto ft_tunnel_shutdown = lib.sym<int (*)(FT_TunnelHandle*)>("ft_tunnel_shutdown");
    auto ft_tunnel_release = lib.sym<void (*)(FT_TunnelHandle*)>("ft_tunnel_release");
    auto ft_job_create = lib.sym<int (*)(const char*, FT_JobHandle**)>("ft_job_create");
    auto ft_job_set_result_cb = lib.sym<int (*)(FT_JobHandle*, ft_job_result_cb, void*)>("ft_job_set_result_cb");
    auto ft_tunnel_start_job = lib.sym<int (*)(FT_TunnelHandle*, FT_JobHandle*)>("ft_tunnel_start_job");
    auto ft_job_get_result = lib.sym<int (*)(FT_JobHandle*, uint32_t, ft_job_result*)>("ft_job_get_result");
    auto ft_job_cancel = lib.sym<int (*)(FT_JobHandle*)>("ft_job_cancel");
    auto ft_job_release = lib.sym<void (*)(FT_JobHandle*)>("ft_job_release");

    ProbeResult out;
    void* agent = create_agent("probe-log");
    if (!agent) fail(agent, destroy_agent, "agent creation failed");
    const std::string config_dir = std::string("probe-config-") + std::to_string(static_cast<long long>(
#if defined(_WIN32)
        GetCurrentProcessId()
#else
        getpid()
#endif
    ));
    if (set_config_dir(agent, config_dir) != 0) fail(agent, destroy_agent, "set config dir failed");

    if (start(agent) != 0) fail(agent, destroy_agent, "agent start failed");
    out.host = get_host(agent);
    if (!contains(out.host, "http://127.0.0.1:")) {
        fail(agent, destroy_agent, "frontend host did not use local webserver");
    }

    unsigned int http_code = 0;
    std::string http_body;
    int token_rc = get_token(agent, "probe-ticket", &http_code, &http_body);
    if (failure_mode) {
        if (token_rc == 0 || http_code != 401 || !contains(http_body, "invalid_plugin_ticket")) {
            fail(agent, destroy_agent, "ticket failure did not map to invalid_plugin_ticket");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            http_body,
            "invalid_plugin_ticket",
            {"secret", "raw-ticket-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
        std::cerr << "invalid_plugin_ticket\n";
        const std::string synthetic_profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant","tenant_name":"Tenant"})";
        if (change_user(agent, synthetic_profile) != 0) fail(agent, destroy_agent, "change_user failed in failure mode");
    } else {
        if (token_rc != 0 || http_code != 200) fail(agent, destroy_agent, "ticket exchange failed");
        if (!contains(http_body, "accessToken") || !contains(http_body, "probe-token")) {
            fail(agent, destroy_agent, "ticket exchange did not return Studio token fields");
        }
        std::string profile_body;
        int profile_rc = get_profile(agent, "probe-token", &http_code, &profile_body);
        if (profile_rc != 0 || http_code != 200) fail(agent, destroy_agent, "profile retrieval failed");
        if (!contains(profile_body, "uidStr") || !contains(profile_body, "probe-user") || !contains(profile_body, "Probe User")) {
            fail(agent, destroy_agent, "profile retrieval did not return stored profile content");
        }
        if (change_user(agent, profile_body) != 0) fail(agent, destroy_agent, "change_user failed");
        if (!is_user_login(agent)) fail(agent, destroy_agent, "expected user login before hub switch");
        switch_local_hub_target(out.host, "https://switched-hub.example.test");
        if (is_user_login(agent)) fail(agent, destroy_agent, "hub switch did not clear login state");
        if (contains(build_login_info(agent), "probe-token") || contains(build_login_cmd(agent), "probe-token")) {
            fail(agent, destroy_agent, "hub switch left old token in login envelope");
        }
        const char* original_hub = std::getenv("PANDAR_PLUGIN_HUB_URL");
        if (!original_hub || original_hub[0] == '\0') {
            fail(agent, destroy_agent, "missing original hub URL env");
        }
        switch_local_hub_target(out.host, original_hub);
        if (get_token(agent, "probe-ticket", &http_code, &http_body) != 0 || http_code != 200) {
            fail(agent, destroy_agent, "ticket exchange after hub switch recovery failed");
        }
        std::string recovered_profile;
        if (get_profile(agent, "probe-token", &http_code, &recovered_profile) != 0 || http_code != 200) {
            fail(agent, destroy_agent, "profile retrieval after hub switch recovery failed");
        }
        if (change_user(agent, recovered_profile) != 0) fail(agent, destroy_agent, "change_user after hub switch recovery failed");
    }

    out.login_command = build_login_cmd(agent);
    out.login_info = build_login_info(agent);
    if (!contains(out.login_command, "studio_userlogin") || !contains(out.login_info, "studio_userlogin")) {
        fail(agent, destroy_agent, "login envelopes lacked studio_userlogin");
    }

    std::string printers_body;
    out.printer_rc = get_print_info(agent, &http_code, &printers_body);
    if (failure_mode) {
        if (out.printer_rc == 0 || http_code != 401 || !contains(printers_body, "invalid_auth_token")) {
            fail(agent, destroy_agent, "printer failure did not map to invalid_auth_token");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            printers_body,
            "invalid_auth_token",
            {"secret", "raw-auth-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
        std::cerr << "invalid_auth_token\n";
    } else if (out.printer_rc != 0 || http_code != 200) {
        fail(agent, destroy_agent, "printer listing failed");
    } else if (get_selected_machine(agent) != "printer-1") {
        fail(agent, destroy_agent, "printer listing did not seed Studio selected machine");
    }

    if (!failure_mode) {
        void* restored_agent = create_agent("probe-log-restored");
        if (!restored_agent) fail(agent, destroy_agent, "restored agent creation failed");
        if (set_config_dir(restored_agent, config_dir) != 0) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored set config dir failed");
        }
        out.restored_login = is_user_login(restored_agent);
        if (!out.restored_login || !contains(build_login_cmd(restored_agent), "probe-token")) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored agent did not reuse persisted login");
        }
        unsigned int restored_http_code = 0;
        std::string restored_body;
        if (get_print_info(restored_agent, &restored_http_code, &restored_body) != 0 || restored_http_code != 200) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored agent could not fetch printer list");
        }
        destroy_agent(restored_agent);
    }

    BBL::TaskQueryParams query;
    query.dev_id = "printer-1";
    std::string tasks_body;
    if (!failure_mode) {
        out.tasks_rc = get_tasks(agent, query, &tasks_body);
        if (out.tasks_rc != 0) fail(agent, destroy_agent, "task listing failed");
    } else {
        out.tasks_rc = -1;
    }

    BBL::PrintParams params;
    params.dev_id = "printer-1";
    params.task_name = "probe.3mf";
    params.project_name = "wrong-display-name.3mf";
    params.filename = argv[2];
    params.plate_index = 1;
    params.task_use_ams = true;
    params.task_flow_cali = false;
    params.task_record_timelapse = false;

    BBL::OnUpdateStatusFn update = [&out](int, int, std::string body) {
        out.update_body = std::move(body);
    };
    BBL::WasCancelledFn cancelled = [] { return false; };
    BBL::OnWaitFn wait = [](int, std::string) { return true; };
    out.print_rc = start_print(agent, params, update, cancelled, wait);
    if (failure_mode) {
        if (out.print_rc == 0 || !contains(out.update_body, "plugin_forbidden")) {
            fail(agent, destroy_agent, "print failure did not map to plugin_forbidden");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            out.update_body,
            "plugin_forbidden",
            {"secret", "raw-forbidden-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
    } else if (out.print_rc != 0) {
        fail(agent, destroy_agent, "print submission failed");
    }

    BBL::OnUpdateStatusFn sdcard_update = [&out](int, int, std::string body) {
        out.sdcard_update_body = std::move(body);
    };
    int sdcard_rc = start_sdcard_print(agent, params, sdcard_update, cancelled, wait);
    if (sdcard_rc == 0 || !contains(out.sdcard_update_body, "unsupported_file_transfer")) {
        fail(agent, destroy_agent, "SD-card print did not return stable unsupported callback");
    }

    if (set_printer_connected(agent, [&out](std::string dev_id) {
        out.direct_connect_callback = dev_id == "printer-1";
    }) != 0) {
        fail(agent, destroy_agent, "printer connected callback registration failed");
    }
    bool server_connected_callback = false;
    if (set_server_connected(agent, [&server_connected_callback](int status, int reason) {
        server_connected_callback = status == 0 && reason == 0;
    }) != 0) {
        fail(agent, destroy_agent, "server connected callback registration failed");
    }
    if (connect_server(agent) != 0 || !server_connected_callback) {
        fail(agent, destroy_agent, "server connect did not report Studio connection success");
    }
    if (update_cert(agent) != 0) {
        fail(agent, destroy_agent, "certificate update should be a no-op success");
    }
    if (set_local_connect(agent, [&out](int status, std::string dev_id, std::string body) {
        out.local_connect_callback = status == 0 && dev_id == "printer-1" && contains(body, R"("dev_type":"N6")");
    }) != 0) {
        fail(agent, destroy_agent, "local connect callback registration failed");
    }
    std::atomic<int> message_count{0};
    if (set_message(agent, [&out, &message_count, agent, destroy_agent](std::string dev_id, std::string body) {
        if (dev_id == "printer-1" && contains(body, R"("command":"get_version")")) {
            if (!contains(body, R"("sequence_id":"20001")") ||
                !contains(body, R"("module")") ||
                !contains(body, R"("name":"ota")") ||
                !contains(body, R"("product_name":"N6")")) {
                fail(agent, destroy_agent, "Studio get_version response did not include version modules");
            }
            out.version_callback = true;
            return;
        }
        if (dev_id != "printer-1" || !contains(body, R"("command":"push_status")")) return;
        if (!contains(body, R"("ipcam")") || !contains(body, R"("local":"rtsps")")) return;
        if (!contains(body, R"("nozzle_temper":28)") ||
            !contains(body, R"("nozzle_target_temper":220)") ||
            !contains(body, R"("nozzle_temper2":27)") ||
            !contains(body, R"("nozzle_target_temper2":215)") ||
            !contains(body, R"("bed_temper":60)") ||
            !contains(body, R"("bed_target_temper":65)") ||
            !contains(body, R"("chamber_temper":32)") ||
            !contains(body, R"("printer_type":"N6")") ||
            !contains(body, R"("support_chamber":true)") ||
            !contains(body, R"("support_chamber_temp_display":true)") ||
            !contains(body, R"("cfg":"")") ||
            !contains(body, R"("fun":"")") ||
            !contains(body, R"("aux":"")") ||
            !contains(body, R"("stat":"")") ||
            !contains(body, R"("device")") ||
            !contains(body, R"("type":1)") ||
            !contains(body, R"("extruder")") ||
            !contains(body, R"("state":18)") ||
            !contains(body, R"({"id":1,"info":8,"temp":14417948,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0})") ||
            !contains(body, R"({"id":0,"info":8,"temp":14090267,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0})") ||
            !contains(body, R"("ams_exist_bits":"1")") ||
            !contains(body, R"("tray_exist_bits":"1")") ||
            !contains(body, R"("tray_now":"0")") ||
            !contains(body, R"("humidity":"3")") ||
            !contains(body, R"("humidity_raw":"25")") ||
            !contains(body, R"("temp":"28.5")") ||
            !contains(body, R"("tray_type":"PLA")") ||
            !contains(body, R"("tray_info_idx":"GFL99")") ||
            !contains(body, R"("vir_slot":[)") ||
            !contains(body, R"("id":"254")") ||
            !contains(body, R"("tray_type":"PETG")") ||
            !contains(body, R"("lights_report":[{"node":"chamber_light","mode":"on"}])")) {
            fail(agent, destroy_agent, "Studio push status did not include plugin printer telemetry");
        }
        out.message_callback = true;
        ++message_count;
    }) != 0) {
        fail(agent, destroy_agent, "message callback registration failed");
    }

    int before_messages = message_count.load();
    if (send_cloud(agent, "printer-1", R"({"pushing":{"command":"pushall","sequence_id":"20000","version":1,"push_target":1}})", 0, 0) != 0) {
        fail(agent, destroy_agent, "cloud pushall request failed");
    }
    if (message_count.load() == before_messages) {
        fail(agent, destroy_agent, "cloud pushall request did not emit Studio push status");
    }
    if (send_cloud(agent, "printer-1", R"({"info":{"command":"get_version","sequence_id":"20001"}})", 0, 0) != 0) {
        fail(agent, destroy_agent, "cloud get_version request failed");
    }
    if (!out.version_callback) {
        fail(agent, destroy_agent, "cloud get_version request did not emit Studio version info");
    }
    before_messages = message_count.load();
    if (start_subscribe(agent, "app") != 0) {
        fail(agent, destroy_agent, "cloud printer module subscription failed");
    }
    out.subscribe_messages = message_count.load() - before_messages;
    if (out.subscribe_messages == 0) {
        fail(agent, destroy_agent, "module subscription did not emit Studio push status for listed printers");
    }
    before_messages = message_count.load();
    if (set_selected_machine(agent, "printer-1") != 0) {
        fail(agent, destroy_agent, "selected cloud printer failed");
    }
    out.selected_machine_messages = message_count.load() - before_messages;
    if (out.selected_machine_messages == 0) {
        fail(agent, destroy_agent, "selected cloud printer did not emit Studio push status");
    }
    before_messages = message_count.load();
    if (add_subscribe(agent, {"printer-1"}) != 0) {
        fail(agent, destroy_agent, "cloud printer subscription failed");
    }
    const auto add_subscribe_messages = message_count.load() - before_messages;
    out.subscribe_messages += add_subscribe_messages;
    if (add_subscribe_messages == 0) {
        fail(agent, destroy_agent, "cloud printer subscription did not emit Studio push status");
    }
    before_messages = message_count.load();
    const auto heartbeat_deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
    while (message_count.load() == before_messages && std::chrono::steady_clock::now() < heartbeat_deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    }
    out.heartbeat_messages = message_count.load() - before_messages;
    if (out.heartbeat_messages == 0) {
        fail(agent, destroy_agent, "cloud printer subscription did not keep Studio push status alive");
    }
    before_messages = message_count.load();
    out.direct_connect_rc = connect_printer(agent, "printer-1", "127.0.0.1", "user", "pass", false);
    out.connect_messages = message_count.load() - before_messages;
    out.direct_message_rc = send_printer(agent, "printer-1", "G28 X", 0, 0);
    if (out.direct_connect_rc != 0 || !out.direct_connect_callback || !out.local_connect_callback || !out.message_callback || out.connect_messages == 0) {
        fail(agent, destroy_agent, "direct printer connect did not report Studio connection success");
    }
    if (failure_mode) {
        if (out.direct_message_rc == 0) {
            fail(agent, destroy_agent, "direct printer message unexpectedly succeeded in failure mode");
        }
    } else if (out.direct_message_rc != 0) {
        fail(agent, destroy_agent, "direct printer message did not submit operation");
    }

    if (get_camera_url(agent, "printer-1|devver|\"tutk\"", [&out](std::string url) {
        out.camera_url = std::move(url);
    }) != 0) {
        fail(agent, destroy_agent, "camera URL lookup failed");
    }
    if (!failure_mode && !contains(out.camera_url, "bambu:///rtsps___bblp:12345678@192.0.2.10/streaming/live/1?proto=rtsps")) {
        fail(agent, destroy_agent, "camera URL did not use Studio RTSPS camera URL");
    }

    out.ft_abi_version = ft_abi_version();
    FT_TunnelHandle* tunnel = nullptr;
    FT_JobHandle* job = nullptr;
    std::string ft_msg;
    int cb_result_ec = 0;
    ft_job_result job_result{};
    if (out.ft_abi_version != 1) fail(agent, destroy_agent, "unexpected ft ABI version");
    if (ft_tunnel_create("ft://probe", &tunnel) != kFtOk || !tunnel) fail(agent, destroy_agent, "ft tunnel create failed");
    out.ft_start_connect_rc = ft_tunnel_start_connect(tunnel, ft_connect_cb, &ft_msg);
    if (out.ft_start_connect_rc != kFtOk || !contains(ft_msg, "unsupported_file_transfer")) {
        fail(agent, destroy_agent, "ft start connect did not return unsupported callback");
    }
    out.ft_sync_rc = ft_tunnel_sync_connect(tunnel);
    if (out.ft_sync_rc != kFtEio) fail(agent, destroy_agent, "ft sync did not return FT_EIO");
    if (ft_job_create(R"({"op":"probe"})", &job) != kFtOk || !job) fail(agent, destroy_agent, "ft job create failed");
    if (ft_job_set_result_cb(job, ft_result_cb, &cb_result_ec) != kFtOk) fail(agent, destroy_agent, "ft result callback registration failed");
    out.ft_start_job_rc = ft_tunnel_start_job(tunnel, job);
    if (out.ft_start_job_rc != kFtOk) fail(agent, destroy_agent, "ft start job failed");
    if (ft_job_get_result(job, 1000, &job_result) != kFtOk || job_result.ec != kFtEio || cb_result_ec != kFtEio) {
        fail(agent, destroy_agent, "ft job result did not report FT_EIO");
    }
    out.ft_job_result_ec = job_result.ec;
    out.ft_cancel_rc = ft_job_cancel(job);
    if (out.ft_cancel_rc != kFtOk) fail(agent, destroy_agent, "ft cancel failed");
    if (ft_tunnel_shutdown(tunnel) != kFtOk) fail(agent, destroy_agent, "ft shutdown failed");
    ft_job_release(job);
    ft_tunnel_release(tunnel);

    if (user_logout(agent, true) != 0) fail(agent, destroy_agent, "logout failed");
    out.logout_command = build_logout_cmd(agent);
    if (!contains(out.logout_command, "studio_useroffline")) {
        fail(agent, destroy_agent, "logout envelope lacked studio_useroffline");
    }
    destroy_agent(agent);
    print_json(out);
    return 0;
}
