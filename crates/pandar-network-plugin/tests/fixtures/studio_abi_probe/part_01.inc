#include <cstdint>
#include <cstdlib>
#include <atomic>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <map>
#include <mutex>
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
using OnUserLoginFn = std::function<void(int, bool)>;
using OnHttpErrorFn = std::function<void(unsigned, std::string)>;
using GetCountryCodeFn = std::function<std::string()>;
using GetSubscribeFailureFn = std::function<void(std::string)>;
using QueueOnMainFn = std::function<void(std::function<void()>)>;
using OnServerErrFn = std::function<void(std::string, int)>;

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
    std::string slicer_uid;
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

struct PluginHttpResult {
    int32_t status;
    uint32_t http_code;
    uint8_t* body_ptr;
    std::size_t body_len;
    std::size_t body_cap;
};

struct FT_TunnelHandle;
struct FT_JobHandle;

using ft_tunnel_connect_cb = void (*)(void* user, int ok, int err, const char* msg);
using ft_job_result_cb = void (*)(void* user, ft_job_result result);

} // extern "C"

namespace {

constexpr int kFtOk = 0;
constexpr int kFtEio = -3;
constexpr int kBambuConnectFailed = -2;
constexpr int kBambuInvalidResult = -19;

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

void write_stale_login_state(const std::string& config_dir) {
    const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
    if (!hub_url || hub_url[0] == '\0') {
        std::cerr << "missing hub URL env for stale login state\n";
        std::exit(2);
    }
    std::filesystem::create_directories(config_dir);
    std::ofstream file(std::filesystem::path(config_dir) / "pandar-plugin-login.json", std::ios::binary | std::ios::trunc);
    file << "{\"hub_url\":\"" << hub_url << "\",\"token\":\"stale-token\",\"session_kind\":\"no_auth\",\"profile\":{\"token\":\"stale-token\",\"user_id\":\"stale-user\",\"user_name\":\"Stale User\",\"tenant_id\":\"tenant-1\",\"tenant_name\":\"Tenant\"}}";
    if (!file) {
        std::cerr << "failed writing stale login state\n";
        std::exit(2);
    }
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

    explicit Library(const char* path, bool studio_lazy_load = false) {
#if defined(_WIN32)
        (void)studio_lazy_load;
        handle = LoadLibraryA(path);
#elif defined(__linux__)
        handle = dlopen(
            path,
            (studio_lazy_load ? RTLD_LAZY : RTLD_NOW) | RTLD_LOCAL
        );
#else
        (void)studio_lazy_load;
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
