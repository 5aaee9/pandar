#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <sstream>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#if defined(_WIN32)
#define PANDAR_ABI extern "C" __declspec(dllexport)
#else
#define PANDAR_ABI extern "C" __attribute__((visibility("default")))
#endif

#if defined(__cplusplus) && defined(__clang__)
#define PANDAR_IGNORE_CXX_LINKAGE_BEGIN _Pragma("clang diagnostic push") _Pragma("clang diagnostic ignored \"-Wreturn-type-c-linkage\"")
#define PANDAR_IGNORE_CXX_LINKAGE_END _Pragma("clang diagnostic pop")
#else
#define PANDAR_IGNORE_CXX_LINKAGE_BEGIN
#define PANDAR_IGNORE_CXX_LINKAGE_END
#endif

namespace BBL {

constexpr int BAMBU_NETWORK_SUCCESS = 0;
constexpr int BAMBU_NETWORK_ERR_INVALID_HANDLE = -1;
constexpr int BAMBU_NETWORK_ERR_CONNECT_FAILED = -2;
constexpr int BAMBU_NETWORK_ERR_INVALID_RESULT = -19;
constexpr int BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED = -11;
constexpr int BAMBU_NETWORK_ERR_QUERY_BIND_INFO_FAILED = -12;
constexpr int BAMBU_NETWORK_ERR_MODIFY_PRINTER_NAME_FAILED = -13;
constexpr int BAMBU_NETWORK_ERR_GET_FILAMENTS_FAILED = -27;
constexpr int BAMBU_NETWORK_ERR_CREATE_FILAMENT_FAILED = -28;
constexpr int BAMBU_NETWORK_ERR_UPDATE_FILAMENT_FAILED = -29;
constexpr int BAMBU_NETWORK_ERR_DELETE_FILAMENT_FAILED = -30;
constexpr int BAMBU_NETWORK_ERR_GET_FILAMENT_CONFIG_FAILED = -31;
constexpr int BAMBU_NETWORK_ERR_BIND_FAILED = -5;
constexpr int BAMBU_NETWORK_ERR_UNBIND_FAILED = -6;
constexpr int BAMBU_NETWORK_ERR_PUT_SETTING_FAILED = -8;
constexpr int BAMBU_NETWORK_ERR_DEL_SETTING_FAILED = -10;
constexpr int BAMBU_NETWORK_ERR_GET_INSTANCE_ID_FAILED = -25;
constexpr int BAMBU_NETWORK_ERR_GET_RATING_ID_FAILED = -21;
constexpr int PrintingStageERROR = 7;
constexpr int PrintingStageFinished = 6;

using OnUserLoginFn = std::function<void(int, bool)>;
using OnPrinterConnectedFn = std::function<void(std::string)>;
using OnLocalConnectedFn = std::function<void(int, std::string, std::string)>;
using OnServerConnectedFn = std::function<void(int, int)>;
using OnMessageFn = std::function<void(std::string, std::string)>;
using OnHttpErrorFn = std::function<void(unsigned, std::string)>;
using GetCountryCodeFn = std::function<std::string()>;
using GetSubscribeFailureFn = std::function<void(std::string)>;
using OnMsgArrivedFn = std::function<void(std::string)>;
using QueueOnMainFn = std::function<void(std::function<void()>)>;
using OnServerErrFn = std::function<void(std::string, int)>;
using OnUpdateStatusFn = std::function<void(int, int, std::string)>;
using WasCancelledFn = std::function<bool()>;
using OnWaitFn = std::function<bool(int, std::string)>;
using ProgressFn = std::function<void(int)>;
using CheckFn = std::function<bool(std::map<std::string, std::string>)>;

struct detectResult {
    std::string result_msg;
    std::string command;
    std::string dev_id;
    std::string model_id;
    std::string dev_name;
    std::string version;
    std::string bind_state;
    std::string connect_type;
};

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

struct FilamentQueryParams {
    std::string category;
    std::string status;
    std::string spool_id;
    std::string rfid;
    int offset = 0;
    int limit = 20;
};

struct FilamentDeleteParams {
    std::vector<std::string> ids;
    std::vector<std::string> rfids;
};

struct PublishParams {
    std::string project_name;
    std::string project_3mf_file;
    std::string preset_name;
    std::string project_model_id;
    std::string design_id;
    std::string config_filename;
};

} // namespace BBL

namespace {

extern "C" {
struct PluginHttpResult {
    int32_t status;
    uint32_t http_code;
    uint8_t* body_ptr;
    std::size_t body_len;
    std::size_t body_cap;
};

PluginHttpResult pandar_plugin_exchange_ticket(const uint8_t*, std::size_t, const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_create_no_auth_session(const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_get_printers(const uint8_t*, std::size_t, const uint8_t*, std::size_t);
void* pandar_plugin_printer_refresh_session_create(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
int32_t pandar_plugin_printer_refresh_session_update(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_printer_refresh(void*);
void pandar_plugin_printer_refresh_session_destroy(void*);
PluginHttpResult pandar_plugin_submit_print(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    int64_t,
    bool,
    bool,
    bool,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_submit_printer_operation(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_operation_json_from_gcode(const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_classify_status_request(const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_printer_telemetry_json(const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_start_local_webserver(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    bool,
    bool
);
PluginHttpResult pandar_plugin_local_webserver_config();
void pandar_plugin_free(void*, std::size_t);
void pandar_plugin_free_with_capacity(void*, std::size_t, std::size_t);
}

enum class MessageTunnel { Cloud, Local };

constexpr int32_t kParseOperation = 0;
constexpr int32_t kParseInvalidNative = 2;
constexpr int32_t kStatusRequestGetVersion = 1;
constexpr int32_t kStatusRequestPushAll = 2;

struct Agent {
    explicit Agent(std::string log_dir_value) : log_dir(std::move(log_dir_value)) {}

    std::string log_dir;
    std::string config_dir;
    std::string cert_folder;
    std::string cert_filename;
    std::string country_code;
    std::string selected_machine;
    std::string active_local_device;
    std::string token;
    std::string user_id;
    std::string user_name;
    std::string avatar;
    std::string profile_json;
    std::string hub_url = "http://127.0.0.1:8080";
    std::string frontend_url = "http://localhost:3000";
    std::string last_error;
    void* printer_refresh_session = nullptr;
    mutable std::mutex trace_mutex;
    mutable std::mutex status_mutex;
    std::map<std::string, std::pair<std::string, std::string>> printer_connections;
    std::map<std::string, std::string> pandar_printer_ids;
    std::map<std::string, std::string> printer_models;
    std::map<std::string, std::string> printer_telemetry;
    std::set<std::string> cloud_subscribed_devices;
    std::set<std::string> cloud_initialized_devices;
    std::map<std::string, std::chrono::steady_clock::time_point> cloud_connection_notifications;
    BBL::OnPrinterConnectedFn on_printer_connected;
    BBL::OnServerConnectedFn on_server_connected;
    BBL::OnLocalConnectedFn on_local_connect;
    BBL::OnMessageFn on_message;
    BBL::OnMessageFn on_local_message;
    std::thread status_thread;
    std::atomic<bool> status_thread_stop = false;
    std::atomic<std::uint64_t> printer_status_epoch = 0;
    bool connected = false;
    bool hub_configured = false;
    bool frontend_configured = false;
};

Agent* as_agent(void* raw) {
    return reinterpret_cast<Agent*>(raw);
}

void trace_plugin_event(const Agent* agent, const std::string& message) {
    if (!agent) return;
    std::filesystem::path base;
    {
        std::lock_guard<std::mutex> lock(agent->trace_mutex);
        base = !agent->config_dir.empty() ? std::filesystem::path(agent->config_dir)
                                          : std::filesystem::path(agent->log_dir);
    }
    if (base.empty()) return;
    std::error_code ec;
    std::filesystem::create_directories(base, ec);
    std::ofstream out(base / "pandar-network-plugin.trace.log", std::ios::app);
    if (out) out << message << '\n';
}

void trace_plugin_event(const Agent* agent, const std::string& event, const std::string& dev_id) {
    trace_plugin_event(agent, event + " dev_id=" + dev_id);
}

std::string body_from_result(PluginHttpResult result);
void refresh_local_webserver_config(Agent* agent);
PluginHttpResult rust_get_printers(const Agent* agent);
PluginHttpResult get_printers_with_token_refresh(Agent* agent, std::uint64_t& request_epoch);
void sync_printer_refresh_session(Agent* agent);

bool has_hub(const Agent* agent) {
    return agent && !agent->hub_url.empty();
}

void clear_login_state(Agent* agent) {
    agent->printer_status_epoch.fetch_add(1);
    agent->token.clear();
    agent->user_id.clear();
    agent->user_name.clear();
    agent->avatar.clear();
    agent->profile_json.clear();
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        agent->selected_machine.clear();
        agent->printer_connections.clear();
        agent->pandar_printer_ids.clear();
        agent->printer_models.clear();
        agent->printer_telemetry.clear();
        agent->cloud_subscribed_devices.clear();
        agent->cloud_initialized_devices.clear();
        agent->cloud_connection_notifications.clear();
    }
    agent->connected = false;
    sync_printer_refresh_session(agent);
}

std::filesystem::path persisted_login_path(const Agent* agent) {
    if (!agent || agent->config_dir.empty()) return {};
    return std::filesystem::path(agent->config_dir) / "pandar-plugin-login.json";
}

void clear_persisted_login(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty()) return;
    std::error_code ignored;
    std::filesystem::remove(path, ignored);
}

std::pair<std::string, bool> env_or_default(const char* primary, const char* secondary, std::string fallback) {
    if (const char* value = std::getenv(primary); value && value[0] != '\0') {
        return {value, true};
    }
    if (const char* value = std::getenv(secondary); value && value[0] != '\0') {
        return {value, true};
    }
    return {std::move(fallback), false};
}

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

std::string field_from_json(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto quote = json.find_first_not_of(" \t\r\n", colon + 1);
    if (quote == std::string::npos || json[quote] != '"') return {};
    std::string out;
    for (std::size_t i = quote + 1; i < json.size(); ++i) {
        const char c = json[i];
        if (c == '\\' && i + 1 < json.size()) {
            out.push_back(json[++i]);
            continue;
        }
        if (c == '"') break;
        out.push_back(c);
    }
    return out;
}

std::vector<std::string> objects_from_array(const std::string& json, const char* key);
std::string object_from_json(const std::string& json, const char* key);

std::string printer_telemetry_from_json(const std::string& printer) {
    auto result = pandar_plugin_printer_telemetry_json(
        reinterpret_cast<const uint8_t*>(printer.data()),
        printer.size()
    );
    return body_from_result(result);
}

std::string studio_dev_id(std::string dev_id) {
    if (const auto separator = dev_id.find('|'); separator != std::string::npos) {
        dev_id.resize(separator);
    }
    return dev_id;
}

std::vector<std::string> objects_from_array(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto start = json.find('[', colon + 1);
    if (start == std::string::npos) return {};
    int array_depth = 0;
    int object_depth = 0;
    bool in_string = false;
    bool escaped = false;
    std::size_t object_start = std::string::npos;
    std::vector<std::string> objects;
    for (std::size_t i = start; i < json.size(); ++i) {
        const char c = json[i];
        if (escaped) {
            escaped = false;
            continue;
        }
        if (c == '\\' && in_string) {
            escaped = true;
            continue;
        }
        if (c == '"') {
            in_string = !in_string;
            continue;
        }
        if (in_string) continue;
        if (c == '[') {
            ++array_depth;
            continue;
        }
        if (c == ']') {
            --array_depth;
            if (array_depth == 0) break;
            continue;
        }
        if (array_depth != 1) continue;
        if (c == '{') {
            if (object_depth == 0) object_start = i;
            ++object_depth;
            continue;
        }
        if (c == '}') {
            --object_depth;
            if (object_depth == 0 && object_start != std::string::npos) {
                objects.push_back(json.substr(object_start, i - object_start + 1));
                object_start = std::string::npos;
            }
        }
    }
    return objects;
}

bool remember_printer_connections(
    Agent* agent,
    const std::string& body,
    std::uint64_t expected_epoch
) {
    if (!agent) return false;
    std::map<std::string, std::pair<std::string, std::string>> printer_connections;
    std::map<std::string, std::string> pandar_printer_ids;
    std::map<std::string, std::string> printer_models;
    std::map<std::string, std::string> printer_telemetry;
    for (const auto& printer : objects_from_array(body, "devices")) {
        const auto dev_id = field_from_json(printer, "dev_id");
        if (dev_id.empty()) continue;
        printer_connections[dev_id] = {
            field_from_json(printer, "dev_ip"),
            field_from_json(printer, "dev_access_code"),
        };
        if (const auto pandar_id = field_from_json(printer, "pandar_printer_id"); !pandar_id.empty()) {
            pandar_printer_ids[dev_id] = pandar_id;
        }
        if (const auto model = field_from_json(printer, "dev_model_name"); !model.empty()) {
            printer_models[dev_id] = model;
        }
        printer_telemetry[dev_id] = printer_telemetry_from_json(printer);
    }
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (agent->printer_status_epoch.load() != expected_epoch) {
            return false;
        }
        agent->printer_connections.swap(printer_connections);
        agent->pandar_printer_ids.swap(pandar_printer_ids);
        agent->printer_models.swap(printer_models);
        agent->printer_telemetry.swap(printer_telemetry);
    }
    return true;
}

bool refresh_printer_status_cache(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return false;
    const auto epoch = agent->printer_status_epoch.load();
    auto result = pandar_plugin_printer_refresh(agent->printer_refresh_session);
    const auto status = result.status;
    const auto http_code = result.http_code;
    auto body = body_from_result(result);
    if (status != 0) {
        trace_plugin_event(
            agent,
            "printer status refresh failed status=" + std::to_string(status) +
                " http_code=" + std::to_string(http_code) + " body=" + body
        );
        return false;
    }
    return remember_printer_connections(agent, body, epoch);
}

std::pair<std::string, std::string> printer_connection_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return {};
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_connections.find(studio_dev_id(dev_id)); it != agent->printer_connections.end()) {
        return it->second;
    }
    return {};
}

std::string pandar_printer_id_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return studio_dev_id(dev_id);
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    const auto normalized = studio_dev_id(dev_id);
    if (const auto it = agent->pandar_printer_ids.find(normalized); it != agent->pandar_printer_ids.end()) {
        return it->second;
    }
    return normalized;
}

std::string printer_model_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return "C11";
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_models.find(studio_dev_id(dev_id)); it != agent->printer_models.end()) {
        return it->second;
    }
    return "C11";
}

std::string first_known_printer_id(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (!agent->printer_connections.empty()) return agent->printer_connections.begin()->first;
    if (!agent->cloud_subscribed_devices.empty()) return *agent->cloud_subscribed_devices.begin();
    return {};
}

std::string ensure_selected_machine(Agent* agent) {
    if (!agent) return {};
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (!agent->selected_machine.empty()) return agent->selected_machine;
    }

    auto selected = first_known_printer_id(agent);
    if (selected.empty() && !agent->token.empty()) {
        refresh_local_webserver_config(agent);
        std::uint64_t request_epoch = 0;
        auto result = get_printers_with_token_refresh(agent, request_epoch);
        auto body = body_from_result(result);
        if (result.status == 0 && remember_printer_connections(agent, body, request_epoch)) {
            selected = first_known_printer_id(agent);
        }
    }

    if (!selected.empty()) {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (agent->selected_machine.empty()) agent->selected_machine = selected;
        agent->cloud_subscribed_devices.insert(selected);
        return agent->selected_machine;
    }
    return {};
}

std::string printer_telemetry_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return printer_telemetry_from_json({});
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_telemetry.find(studio_dev_id(dev_id)); it != agent->printer_telemetry.end()) {
        return it->second;
    }
    return printer_telemetry_from_json({});
}

std::uint32_t studio_ip_integer(const std::string& host) {
    unsigned a = 0;
    unsigned b = 0;
    unsigned c = 0;
    unsigned d = 0;
    char dot1 = 0;
    char dot2 = 0;
    char dot3 = 0;
    std::istringstream stream(host);
    if (!(stream >> a >> dot1 >> b >> dot2 >> c >> dot3 >> d)) return 0;
    if (dot1 != '.' || dot2 != '.' || dot3 != '.') return 0;
    if (a > 255 || b > 255 || c > 255 || d > 255) return 0;
    return static_cast<std::uint32_t>(a | (b << 8) | (c << 16) | (d << 24));
}

std::string camera_url_for(const Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty() || agent->hub_url.empty()) return {};
    const auto [host, access_code] = printer_connection_for(agent, dev_id);
    if (!host.empty() && !access_code.empty()) {
        return "bambu:///rtsps___bblp:" + access_code + "@" + host + "/streaming/live/1?proto=rtsps";
    }
    const auto tenant_id = field_from_json(agent->profile_json, "tenant_id");
    if (tenant_id.empty()) return {};
    return agent->hub_url + "/api/v1/tenants/" + tenant_id + "/printers/" + pandar_printer_id_for(agent, dev_id) + "/camera.mp4";
}

std::string printer_push_status_report(const Agent* agent, const std::string& dev_id) {
    const auto [host, access_code] = printer_connection_for(agent, dev_id);
    const auto rtsp_url = "rtsps://bblp:" + access_code + "@" + host + "/streaming/live/1";
    const auto ip = studio_ip_integer(host);
    return std::string(R"({"print":{"command":"push_status","msg":0,)")
        + printer_telemetry_for(agent, dev_id) +
        R"(,"cfg":"","fun":"","aux":"","stat":"","wifi_signal":"100%","sdcard":true,"ipcam":{"ipcam_dev":"1","liveview":{"local":"rtsps","remote":"none"},"rtsp_url":)" +
        escape_json(rtsp_url) + R"(},"net":{"info":[{"ip":)" + std::to_string(ip) + R"(}]}}})";
}

std::string printer_version_report(const Agent* agent, const std::string& dev_id, const std::string& sequence_id) {
    const auto model = printer_model_for(agent, dev_id);
    const auto product_name = model.empty() ? "Bambu Lab" : model;
    const auto serial = studio_dev_id(dev_id);
    const auto module = [&](const char* name, const char* sw_ver, const char* hw_ver) {
        return std::string(R"({"name":)") + escape_json(name) +
            R"(,"product_name":)" + escape_json(product_name) +
            R"(,"sw_ver":)" + escape_json(sw_ver) +
            R"(,"sw_new_ver":"","hw_ver":)" + escape_json(hw_ver) +
            R"(,"sn":)" + escape_json(serial) + R"(,"flag":0})";
    };
    return std::string(R"({"info":{"command":"get_version","sequence_id":)") +
        escape_json(sequence_id.empty() ? "0" : sequence_id) +
        R"(,"module":[)" +
        module("ota", "01.07.00.00", "OTA") + "," +
        module("esp32", "01.07.22.25", "AP05") + "," +
        module("rv1126", "00.00.27.38", "AP05") + "," +
        module("th", "00.00.04.00", "TH07") + "," +
        module("mc", "00.00.10.00", "MC07") +
        R"(]}})";
}

std::string printer_alive_report(const Agent* agent, const std::string& dev_id) {
    const auto [host, _] = printer_connection_for(agent, dev_id);
    const auto model = printer_model_for(agent, dev_id);
    return std::string(R"({"dev_name":)") + escape_json(dev_id) +
        R"(,"dev_id":)" + escape_json(studio_dev_id(dev_id)) +
        R"(,"dev_ip":)" + escape_json(host) +
        R"(,"dev_type":)" + escape_json(model) +
        R"(,"dev_signal":"100%","connect_type":"lan","bind_state":"free","sec_link":"secure","ssdp_version":"1"})";
}

void emit_cloud_printer_connected_signal(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return;
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto now = std::chrono::steady_clock::now();
    BBL::OnPrinterConnectedFn on_printer_connected;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (!agent->on_printer_connected ||
            agent->cloud_initialized_devices.find(normalized_dev_id) !=
                agent->cloud_initialized_devices.end()) {
            return;
        }
        if (const auto notification = agent->cloud_connection_notifications.find(normalized_dev_id);
            notification != agent->cloud_connection_notifications.end() &&
            now - notification->second < std::chrono::seconds(1)) {
            return;
        }
        agent->cloud_connection_notifications[normalized_dev_id] = now;
        on_printer_connected = agent->on_printer_connected;
    }
    const auto topic = std::string("tunnel/") + normalized_dev_id;
    trace_plugin_event(agent, "printer_connected callback=1", topic);
    on_printer_connected(topic);
}

void emit_printer_status(Agent* agent, const std::string& dev_id, MessageTunnel tunnel) {
    if (!agent || dev_id.empty()) return;
    const auto report = printer_push_status_report(agent, dev_id);
    trace_plugin_event(agent, "push_status", dev_id);
    BBL::OnMessageFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = tunnel == MessageTunnel::Cloud ? agent->on_message : agent->on_local_message;
    }
    trace_plugin_event(
        agent,
        std::string("push_status callbacks dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (callback ? "1" : "0"));
    if (callback) callback(dev_id, report);
}

void emit_printer_version(
    Agent* agent,
    const std::string& dev_id,
    const std::string& sequence_id,
    MessageTunnel tunnel
) {
    if (!agent || dev_id.empty()) return;
    BBL::OnMessageFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = tunnel == MessageTunnel::Cloud ? agent->on_message : agent->on_local_message;
    }
    trace_plugin_event(
        agent,
        std::string("get_version_response dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (callback ? "1" : "0"));
    if (callback) callback(dev_id, printer_version_report(agent, dev_id, sequence_id));
}

void emit_cloud_printer_connected_status(Agent* agent, const std::string& dev_id) {
    emit_cloud_printer_connected_signal(agent, dev_id);
    emit_printer_status(agent, dev_id, MessageTunnel::Cloud);
}

void emit_cloud_printer_connected_statuses(Agent* agent, const std::vector<std::string>& dev_ids) {
    for (const auto& dev_id : dev_ids) {
        emit_cloud_printer_connected_status(agent, dev_id);
    }
}

void emit_local_connect(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return;
    BBL::OnLocalConnectedFn on_local_connect;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        on_local_connect = agent->on_local_connect;
    }
    trace_plugin_event(
        agent,
        std::string("local_connect dev_id=") + dev_id +
            " callback=" + (on_local_connect ? "1" : "0"));
    if (on_local_connect) on_local_connect(0, dev_id, printer_alive_report(agent, dev_id));
}

bool handle_status_request(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel
) {
    auto request = pandar_plugin_classify_status_request(
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size()
    );
    const auto request_kind = request.status;
    const auto sequence_id = body_from_result(request);
    if (request_kind == kStatusRequestGetVersion) {
        if (tunnel == MessageTunnel::Cloud) {
            std::lock_guard<std::mutex> lock(agent->status_mutex);
            const auto normalized_dev_id = studio_dev_id(dev_id);
            agent->cloud_initialized_devices.insert(normalized_dev_id);
            agent->cloud_connection_notifications.erase(normalized_dev_id);
        }
        emit_printer_version(
            agent,
            dev_id,
            sequence_id,
            tunnel
        );
        return true;
    }
    if (request_kind == kStatusRequestPushAll) {
        refresh_printer_status_cache(agent);
        if (tunnel == MessageTunnel::Cloud) {
            emit_cloud_printer_connected_signal(agent, dev_id);
        }
        emit_printer_status(agent, dev_id, tunnel);
        return true;
    }
    return false;
}

struct StatusHeartbeatTargets {
    std::vector<std::string> cloud;
    std::string local;
};

StatusHeartbeatTargets status_heartbeat_targets(Agent* agent) {
    StatusHeartbeatTargets targets;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        targets.cloud.assign(
            agent->cloud_subscribed_devices.begin(),
            agent->cloud_subscribed_devices.end()
        );
        targets.local = agent->active_local_device;
    }
    return targets;
}

bool has_status_heartbeat_listener(Agent* agent) {
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    return (!agent->cloud_subscribed_devices.empty() && static_cast<bool>(agent->on_message)) ||
        (!agent->active_local_device.empty() && static_cast<bool>(agent->on_local_message));
}

void start_status_heartbeat(Agent* agent) {
    if (!agent || agent->status_thread.joinable()) return;
    agent->status_thread_stop = false;
    agent->status_thread = std::thread([agent] {
        while (!agent->status_thread_stop.load()) {
            std::this_thread::sleep_for(std::chrono::seconds(2));
            if (agent->status_thread_stop.load()) break;
            if (has_status_heartbeat_listener(agent)) {
                refresh_printer_status_cache(agent);
            }
            auto targets = status_heartbeat_targets(agent);
            emit_cloud_printer_connected_statuses(agent, targets.cloud);
            if (!targets.local.empty()) {
                emit_printer_status(agent, targets.local, MessageTunnel::Local);
            }
        }
    });
}

void stop_status_heartbeat(Agent* agent) {
    if (!agent) return;
    agent->status_thread_stop = true;
    if (agent->status_thread.joinable()) agent->status_thread.join();
}

std::string object_from_json(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto start = json.find('{', colon + 1);
    if (start == std::string::npos) return {};
    int depth = 0;
    bool in_string = false;
    bool escaped = false;
    for (std::size_t i = start; i < json.size(); ++i) {
        const char c = json[i];
        if (escaped) {
            escaped = false;
            continue;
        }
        if (c == '\\' && in_string) {
            escaped = true;
            continue;
        }
        if (c == '"') {
            in_string = !in_string;
            continue;
        }
        if (in_string) continue;
        if (c == '{') ++depth;
        if (c == '}') {
            --depth;
            if (depth == 0) return json.substr(start, i - start + 1);
        }
    }
    return {};
}

void apply_profile_json(Agent* agent, const std::string& json) {
    agent->profile_json = json;
    if (const auto v = field_from_json(json, "token"); !v.empty()) agent->token = v;
    if (const auto v = field_from_json(json, "uidStr"); !v.empty()) agent->user_id = v;
    if (const auto v = field_from_json(json, "name"); !v.empty()) agent->user_name = v;
    if (const auto v = field_from_json(json, "user_id"); !v.empty()) agent->user_id = v;
    if (const auto v = field_from_json(json, "user_name"); !v.empty()) agent->user_name = v;
    if (const auto v = field_from_json(json, "tenant_name"); !v.empty() && agent->user_name.empty()) agent->user_name = v;
    sync_printer_refresh_session(agent);
}

void persist_login_state(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty() || !agent || agent->token.empty() || agent->profile_json.empty()) return;
    std::error_code error;
    std::filesystem::create_directories(path.parent_path(), error);
    if (error) return;

    auto temp_path = path;
    temp_path += ".tmp";
    std::ofstream file(temp_path, std::ios::binary | std::ios::trunc);
    if (!file) return;
    file << "{"
         << "\"hub_url\":" << escape_json(agent->hub_url) << ","
         << "\"token\":" << escape_json(agent->token) << ","
         << "\"profile\":" << agent->profile_json
         << "}";
    file.close();
    if (!file) {
        std::filesystem::remove(temp_path, error);
        return;
    }
    std::filesystem::rename(temp_path, path, error);
    if (error) {
        std::filesystem::remove(path, error);
        std::filesystem::rename(temp_path, path, error);
    }
}

void load_persisted_login(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty() || !agent || !agent->token.empty()) return;
    std::ifstream file(path, std::ios::binary);
    if (!file) return;
    std::string body((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    if (field_from_json(body, "hub_url") != agent->hub_url) return;
    const auto token = field_from_json(body, "token");
    const auto profile = object_from_json(body, "profile");
    if (token.empty() || profile.empty()) return;
    agent->token = token;
    apply_profile_json(agent, profile);
    if (agent->token.empty()) agent->token = token;
}

std::string studio_token_body(const Agent* agent) {
    const auto token = agent ? agent->token : std::string{};
    return std::string("{") +
           "\"accessToken\":" + escape_json(token) + "," +
           "\"refreshToken\":\"\"," +
           "\"expiresIn\":31536000," +
           "\"refreshExpiresIn\":31536000," +
           "\"tfaKey\":\"\"," +
           "\"accessMethod\":\"pandar\"," +
           "\"loginType\":\"pandar\"" +
           "}";
}

std::string studio_profile_body(const Agent* agent) {
    if (!agent) return R"({"uidStr":"","account":"","name":"","avatar":""})";
    return std::string("{") +
           "\"uidStr\":" + escape_json(agent->user_id) + "," +
           "\"account\":" + escape_json(agent->user_name) + "," +
           "\"name\":" + escape_json(agent->user_name) + "," +
           "\"avatar\":" + escape_json(agent->avatar) +
           "}";
}

std::string body_from_result(PluginHttpResult result) {
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
        pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    }
    return body;
}

void sync_printer_refresh_session(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return;
    pandar_plugin_printer_refresh_session_update(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
}

PluginHttpResult rust_exchange_ticket(const Agent* agent, const std::string& ticket) {
    return pandar_plugin_exchange_ticket(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(ticket.data()),
        ticket.size()
    );
}

PluginHttpResult rust_create_no_auth_session(const Agent* agent) {
    return pandar_plugin_create_no_auth_session(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size()
    );
}

PluginHttpResult rust_start_local_webserver(const Agent* agent) {
    return pandar_plugin_start_local_webserver(
        reinterpret_cast<const uint8_t*>(agent->frontend_url.data()),
        agent->frontend_url.size(),
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        agent->frontend_configured,
        agent->hub_configured
    );
}

void refresh_local_webserver_config(Agent* agent) {
    auto result = pandar_plugin_local_webserver_config();
    std::string body = body_from_result(result);
    if (result.status != 0) return;
    if (const auto hub_url = field_from_json(body, "hub_url"); !hub_url.empty()) {
        if (hub_url != agent->hub_url) {
            clear_persisted_login(agent);
            clear_login_state(agent);
        }
        agent->hub_url = hub_url;
        sync_printer_refresh_session(agent);
    }
}

PluginHttpResult rust_get_printers(const Agent* agent) {
    return pandar_plugin_get_printers(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
}

PluginHttpResult rust_submit_print(const Agent* agent, const BBL::PrintParams& params) {
    const std::string& display_name = params.task_name.empty() ? params.project_name : params.task_name;
    const std::string& artifact_path = params.filename;
    const auto printer_id = pandar_printer_id_for(agent, params.dev_id);
    return pandar_plugin_submit_print(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(display_name.data()),
        display_name.size(),
        reinterpret_cast<const uint8_t*>(artifact_path.data()),
        artifact_path.size(),
        params.plate_index,
        params.task_use_ams,
        params.task_flow_cali,
        params.task_record_timelapse,
        reinterpret_cast<const uint8_t*>(params.ams_mapping.data()),
        params.ams_mapping.size(),
        reinterpret_cast<const uint8_t*>(params.ams_mapping2.data()),
        params.ams_mapping2.size(),
        reinterpret_cast<const uint8_t*>(params.ams_mapping_info.data()),
        params.ams_mapping_info.size()
    );
}

PluginHttpResult rust_operation_json_from_gcode(const std::string& message) {
    return pandar_plugin_operation_json_from_gcode(
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size()
    );
}

PluginHttpResult rust_submit_printer_operation(const Agent* agent, const std::string& printer_id, const std::string& operation_json) {
    return pandar_plugin_submit_printer_operation(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(operation_json.data()),
        operation_json.size()
    );
}

int submit_printer_operation_json(Agent* agent, std::string dev_id, const std::string& operation_json) {
    refresh_local_webserver_config(agent);
    dev_id = pandar_printer_id_for(agent, dev_id);
    if (agent->token.empty() || dev_id.empty()) {
        agent->last_error = R"({"error":"invalid_printer_operation"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }

    auto result = rust_submit_printer_operation(agent, dev_id, operation_json);
    std::string body = body_from_result(result);
    if (result.status != 0) {
        agent->last_error = body;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }

    agent->last_error.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

void apply_login_response_body(Agent* agent, const std::string& body) {
    agent->token = field_from_json(body, "token");
    agent->profile_json = object_from_json(body, "profile");
    apply_profile_json(agent, agent->profile_json);
}

void try_no_auth_session(Agent* agent) {
    if (!agent || !agent->token.empty()) return;
    refresh_local_webserver_config(agent);
    auto result = rust_create_no_auth_session(agent);
    std::string body = body_from_result(result);
    if (result.status != 0) return;
    apply_login_response_body(agent, body);
    persist_login_state(agent);
}

bool result_needs_token_refresh(const PluginHttpResult& result) {
    return result.status != 0 && (result.http_code == 401 || result.http_code == 410);
}

bool refresh_no_auth_session(Agent* agent) {
    if (!agent) return false;
    clear_login_state(agent);
    try_no_auth_session(agent);
    return !agent->token.empty();
}

PluginHttpResult get_printers_with_token_refresh(Agent* agent, std::uint64_t& request_epoch) {
    request_epoch = agent->printer_status_epoch.load();
    auto result = rust_get_printers(agent);
    if (result_needs_token_refresh(result)) {
        body_from_result(result);
        if (refresh_no_auth_session(agent)) {
            request_epoch = agent->printer_status_epoch.load();
            result = rust_get_printers(agent);
        }
    }
    return result;
}

std::string login_envelope(const Agent* agent, bool logout) {
    if (logout || !agent || agent->token.empty()) {
        return R"({"sequence_id":"0","command":"studio_useroffline","data":{}})";
    }
    return std::string(R"({"sequence_id":"0","command":"studio_userlogin","data":{)") +
           "\"avatar\":" + escape_json(agent->avatar) + "," +
           "\"name\":" + escape_json(agent->user_name) + "," +
           "\"user_id\":" + escape_json(agent->user_id) + "," +
           "\"user_name\":" + escape_json(agent->user_name) + "," +
           "\"nickname\":" + escape_json(agent->user_name) + "," +
           "\"account\":" + escape_json(agent->user_name) + "," +
           "\"token\":" + escape_json(agent->token) + "," +
           R"("refresh":""}})";
}

std::string profile_body(const Agent* agent) {
    if (!agent || agent->profile_json.empty()) return R"({"user_id":"","user_name":"","tenant_id":"","tenant_name":""})";
    return agent->profile_json;
}

void success_body(unsigned int* http_code, std::string* http_body, std::string body) {
    if (http_code) *http_code = 200;
    if (http_body) *http_body = std::move(body);
}

} // namespace

PANDAR_IGNORE_CXX_LINKAGE_BEGIN

PANDAR_ABI std::string bambu_network_get_version() {
    return "02.07.01.00";
}

PANDAR_ABI std::string bambu_network_get_user_id(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_id : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_name(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_avatar(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->avatar : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_nickanme(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_build_login_cmd(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_build_logout_cmd(void* agent) {
    return login_envelope(as_agent(agent), true);
}

PANDAR_ABI std::string bambu_network_build_login_info(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_get_bambulab_host(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return {};
    auto result = rust_start_local_webserver(a);
    std::string body = body_from_result(result);
    if (result.status != 0) {
        a->last_error = body;
        return {};
    }
    if (const auto base_url = field_from_json(body, "base_url"); !base_url.empty()) {
        a->last_error.clear();
        return base_url;
    }
    a->last_error = R"({"error":"local_webserver_unavailable"})";
    return {};
}

PANDAR_ABI std::string bambu_network_get_user_selected_machine(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return {};
    auto selected = ensure_selected_machine(a);
    trace_plugin_event(a, std::string("get_user_selected_machine selected=") + selected);
    return selected;
}

PANDAR_ABI std::string bambu_network_get_studio_info_url(void*) {
    return {};
}

PANDAR_ABI std::string bambu_network_request_setting_id(void*, std::string, std::map<std::string, std::string>*, unsigned int* http_code) {
    if (http_code) *http_code = 0;
    return {};
}

PANDAR_IGNORE_CXX_LINKAGE_END

PANDAR_ABI bool bambu_network_check_debug_consistent(bool) {
    return true;
}

PANDAR_ABI void* bambu_network_create_agent(std::string log_dir) {
    auto* agent = new Agent(std::move(log_dir));
    auto [hub_url, hub_configured] = env_or_default("PANDAR_PLUGIN_HUB_URL", "APP_API_URL", "http://127.0.0.1:8080");
    auto [frontend_url, frontend_configured] = env_or_default("PANDAR_PLUGIN_FRONTEND_URL", "APP_BASE_URL", "http://localhost:3000");
    agent->hub_url = std::move(hub_url);
    agent->frontend_url = std::move(frontend_url);
    agent->hub_configured = hub_configured;
    agent->frontend_configured = frontend_configured;
    agent->printer_refresh_session = pandar_plugin_printer_refresh_session_create(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
    start_status_heartbeat(agent);
    return agent;
}

PANDAR_ABI int bambu_network_destroy_agent(void* agent) {
    auto* a = as_agent(agent);
    stop_status_heartbeat(a);
    if (a) pandar_plugin_printer_refresh_session_destroy(a->printer_refresh_session);
    delete a;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_init_log(void*) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_config_dir(void* agent, std::string config_dir) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::mutex> lock(a->trace_mutex);
        a->config_dir = std::move(config_dir);
    }
    load_persisted_login(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_cert_file(void* agent, std::string folder, std::string filename) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->cert_folder = std::move(folder);
    a->cert_filename = std::move(filename);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_country_code(void* agent, std::string country_code) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->country_code = std::move(country_code);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto result = rust_start_local_webserver(a);
    if (result.status != 0) {
        a->last_error = body_from_result(result);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    try_no_auth_session(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

#define PANDAR_CALLBACK_SETTER(name, type) \
    PANDAR_ABI int name(void* agent, BBL::type) { \
        return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE; \
    }

PANDAR_CALLBACK_SETTER(bambu_network_set_on_ssdp_msg_fn, OnMsgArrivedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_login_fn, OnUserLoginFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_http_error_fn, OnHttpErrorFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_get_country_code_fn, GetCountryCodeFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_subscribe_failure_fn, GetSubscribeFailureFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_message_fn, OnMessageFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_queue_on_main_fn, QueueOnMainFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_server_callback, OnServerErrFn)

#undef PANDAR_CALLBACK_SETTER

PANDAR_ABI int bambu_network_set_on_server_connected_fn(void* agent, BBL::OnServerConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_server_connected = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_local_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_printer_connected_fn(void* agent, BBL::OnPrinterConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_printer_connected = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_connect_fn(void* agent, BBL::OnLocalConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_local_connect = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_connect_server(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (a->connected && has_hub(a) && a->last_error.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->cloud_initialized_devices.clear();
        a->cloud_connection_notifications.clear();
    }
    a->connected = has_hub(a);
    if (a->connected) a->last_error.clear();
    BBL::OnServerConnectedFn on_server_connected;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        on_server_connected = a->on_server_connected;
    }
    if (on_server_connected) {
        on_server_connected(a->connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED, 0);
    }
    return a->connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
}

PANDAR_ABI bool bambu_network_is_server_connected(void* agent) {
    auto* a = as_agent(agent);
    return a && a->connected && has_hub(a) && a->last_error.empty();
}

PANDAR_ABI int bambu_network_refresh_connection(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (a->connected && has_hub(a) && a->last_error.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    return bambu_network_connect_server(agent);
}

PANDAR_ABI int bambu_network_start_subscribe(void* agent, std::string) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_stop_subscribe(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_add_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        for (const auto& dev_id : dev_ids) {
            if (!dev_id.empty()) a->cloud_subscribed_devices.insert(studio_dev_id(dev_id));
        }
    }
    emit_cloud_printer_connected_statuses(a, dev_ids);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_del_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    for (const auto& dev_id : dev_ids) {
        const auto normalized_dev_id = studio_dev_id(dev_id);
        a->cloud_subscribed_devices.erase(normalized_dev_id);
        a->cloud_initialized_devices.erase(normalized_dev_id);
        a->cloud_connection_notifications.erase(normalized_dev_id);
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI void bambu_network_enable_multi_machine(void*, bool) {}

PANDAR_ABI int bambu_network_send_message(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message", dev_id);
    if (dev_id.empty()) dev_id = ensure_selected_machine(a);
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    if (handle_status_request(a, dev_id, message, MessageTunnel::Cloud)) {
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    auto parsed = rust_operation_json_from_gcode(message);
    std::string operation_json = body_from_result(parsed);
    if (parsed.status == kParseOperation) {
        return submit_printer_operation_json(a, dev_id, operation_json);
    }
    if (parsed.status == kParseInvalidNative) {
        a->last_error = operation_json;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_connect_printer(void* agent, std::string dev_id, std::string, std::string, std::string, bool) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->active_local_device = studio_dev_id(dev_id);
    }
    emit_local_connect(a, studio_dev_id(dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_disconnect_printer(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->active_local_device.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_send_message_to_printer(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message_to_printer", dev_id);

    if (handle_status_request(a, dev_id, message, MessageTunnel::Local)) {
        return BBL::BAMBU_NETWORK_SUCCESS;
    }

    auto parsed = rust_operation_json_from_gcode(message);
    std::string operation_json = body_from_result(parsed);
    if (parsed.status != kParseOperation) {
        a->last_error = operation_json;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    return submit_printer_operation_json(a, dev_id, operation_json);
}

PANDAR_ABI int bambu_network_update_cert(void* agent) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI void bambu_network_install_device_cert(void*, std::string, bool) {}

PANDAR_ABI bool bambu_network_start_discovery(void*, bool, bool) {
    return false;
}

PANDAR_ABI int bambu_network_change_user(void* agent, std::string user_info) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (user_info.empty() || user_info == "{}") {
        clear_persisted_login(a);
        clear_login_state(a);
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    apply_profile_json(a, user_info);
    persist_login_state(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI bool bambu_network_is_user_login(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a && !a->token.empty();
}

PANDAR_ABI int bambu_network_user_logout(void* agent, bool) {
    auto* a = as_agent(agent);
    if (a) {
        clear_persisted_login(a);
        clear_login_state(a);
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_profile(void* agent, std::string token, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (!token.empty()) {
        a->token = std::move(token);
        sync_printer_refresh_session(a);
    }
    if (a->profile_json.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"profile_unavailable"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    success_body(http_code, http_body, studio_profile_body(a));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_token(void* agent, std::string ticket, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    if (ticket.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_plugin_ticket"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_exchange_ticket(a, ticket);
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
        pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    }
    if (http_code) *http_code = result.http_code;
    if (http_body) *http_body = body;
    if (result.status != 0) {
        a->last_error = body;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    apply_login_response_body(a, body);
    persist_login_state(a);
    a->last_error.clear();
    success_body(http_code, http_body, studio_token_body(a));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_info(void* agent, int* identifier) {
    if (identifier) *identifier = as_agent(agent) ? 1 : 0;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_user_selected_machine(void* agent, std::string dev_id) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, std::string("set_user_selected_machine dev_id=") + dev_id);
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->selected_machine = std::move(dev_id);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_ping_bind(void* agent, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_BIND_FAILED;
}

PANDAR_ABI int bambu_network_bind_detect(void* agent, std::string, std::string, BBL::detectResult& detect) {
    detect = BBL::detectResult{};
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_bind(void* agent, std::string, std::string, std::string, std::string, bool, BBL::OnUpdateStatusFn) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_BIND_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_unbind(void* agent, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_UNBIND_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_request_bind_ticket(void* agent, std::string* ticket) {
    if (ticket) ticket->clear();
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_query_bind_status(void* agent, std::vector<std::string>, unsigned int* http_code, std::string* http_body) {
    if (http_code) *http_code = 0;
    if (http_body) http_body->clear();
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_QUERY_BIND_INFO_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_modify_printer_name(void* agent, std::string, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_MODIFY_PRINTER_NAME_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_report_consent(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "start_print", params.dev_id);
    refresh_local_webserver_config(a);
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    if (a->token.empty() || params.dev_id.empty() || params.filename.empty()) {
        if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, "Pandar plugin print submission is missing token, printer, or artifact");
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_submit_print(a, params);
    std::string body = body_from_result(result);
    trace_plugin_event(a, std::string("start_print result=") + std::to_string(result.status) + " http=" + std::to_string(result.http_code), params.dev_id);
    if (result.status != 0) {
        a->last_error = body;
        if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, body);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    if (update_fn) update_fn(BBL::PrintingStageFinished, BBL::BAMBU_NETWORK_SUCCESS, "3");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_local_print_with_record(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    if (auto* a = as_agent(agent)) trace_plugin_event(a, "start_local_print_with_record", params.dev_id);
    return bambu_network_start_print(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), std::move(wait_fn));
}

PANDAR_ABI int bambu_network_start_send_gcode_to_sdcard(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    if (!as_agent(agent)) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, R"({"error":"unsupported_file_transfer"})");
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    (void)params;
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_start_local_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    if (auto* a = as_agent(agent)) trace_plugin_event(a, "start_local_print", params.dev_id);
    return bambu_network_start_send_gcode_to_sdcard(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), {});
}

PANDAR_ABI int bambu_network_start_sdcard_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    return bambu_network_start_send_gcode_to_sdcard(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), {});
}

PANDAR_ABI int bambu_network_get_user_presets(void*, std::map<std::string, std::map<std::string, std::string>>* user_presets) {
    if (user_presets) user_presets->clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_put_setting(void*, std::string, std::string, std::map<std::string, std::string>*, unsigned int* http_code) {
    if (http_code) *http_code = 0;
    return BBL::BAMBU_NETWORK_ERR_PUT_SETTING_FAILED;
}

PANDAR_ABI int bambu_network_get_setting_list(void*, std::string, BBL::ProgressFn pro_fn, BBL::WasCancelledFn) {
    if (pro_fn) pro_fn(100);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_setting_list2(void*, std::string, BBL::CheckFn, BBL::ProgressFn pro_fn, BBL::WasCancelledFn) {
    if (pro_fn) pro_fn(100);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_delete_setting(void*, std::string) {
    return BBL::BAMBU_NETWORK_ERR_DEL_SETTING_FAILED;
}

PANDAR_ABI int bambu_network_set_extra_http_header(void* agent, std::map<std::string, std::string>) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_get_my_message(void*, int, int, int, unsigned int* http_code, std::string* http_body) {
    success_body(http_code, http_body, "{}");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_check_user_task_report(void*, int* task_id, bool* printable) {
    if (task_id) *task_id = 0;
    if (printable) *printable = false;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_print_info(void* agent, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    if (a->token.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    }
    std::uint64_t request_epoch = 0;
    auto result = get_printers_with_token_refresh(a, request_epoch);
    if (http_code) *http_code = result.http_code;
    auto body = body_from_result(result);
    if (result.status == 0 && !remember_printer_connections(a, body, request_epoch)) {
        trace_plugin_event(a, "get_user_print_info discarded after login change");
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    }
    trace_plugin_event(a, std::string("get_user_print_info status=") + std::to_string(result.status));
    if (http_body) *http_body = body;
    if (result.status != 0) return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_tasks(void* agent, BBL::TaskQueryParams, std::string* http_body) {
    if (!as_agent(agent)) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (http_body) *http_body = R"({"total":0,"hits":[]})";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_printer_firmware(void*, std::string dev_id, unsigned* http_code, std::string* http_body) {
    if (http_code) *http_code = 200;
    if (http_body) *http_body = std::string(R"({"devices":[{"dev_id":)") + escape_json(dev_id) + R"(,"firmware":[],"ams":[]}]})";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_task_plate_index(void*, std::string, int* plate_index) {
    if (plate_index) *plate_index = -1;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_subtask_info(void*, std::string, std::string* task_json, unsigned int* http_code, std::string* http_body) {
    if (task_json) task_json->clear();
    success_body(http_code, http_body, "{}");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_slice_info(void*, std::string, std::string, int, std::string* slice_json) {
    if (slice_json) slice_json->clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_camera_url(void* agent, std::string dev_id, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    if (callback) callback(camera_url_for(a, dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_camera_url_for_golive(void* agent, std::string dev_id, std::string, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    if (callback) callback(camera_url_for(a, dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_hms_snapshot(void*, std::string&, std::string&, std::function<void(std::string, int)> callback) {
    if (callback) callback({}, -1);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_design_staffpick(void*, int, int, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"list":[],"total":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_publish(void*, BBL::PublishParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, std::string* out) {
    if (out) out->clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_model_publish_url(void*, std::string* url) {
    if (url) *url = "https://makerworld.com/";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

class BBLModelTask;

PANDAR_ABI int bambu_network_get_subtask(void*, BBLModelTask*, std::function<void(BBLModelTask*)>) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_model_mall_home_url(void*, std::string* url) {
    if (url) *url = "https://makerworld.com/";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_model_mall_detail_url(void*, std::string* url, std::string id) {
    if (url) *url = std::string("https://makerworld.com/models/") + id;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_put_model_mall_rating(void*, int, int, std::string, std::vector<std::string>, unsigned int& http_code, std::string& http_error) {
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_oss_config(void*, std::string& config, std::string, unsigned int& http_code, std::string& http_error) {
    config.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_put_rating_picture_oss(void*, std::string&, std::string& pic_oss_path, std::string, int, unsigned int& http_code, std::string& http_error) {
    pic_oss_path.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_model_mall_rating(void*, int, std::string& rating_result, unsigned int& http_code, std::string& http_error) {
    rating_result.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_mw_user_preference(void*, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"recommendStatus":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_mw_user_4ulist(void*, int, int, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"list":[],"total":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_filament_spools(void*, BBL::FilamentQueryParams, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_GET_FILAMENTS_FAILED;
}

PANDAR_ABI int bambu_network_create_filament_spool(void*, std::string, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_CREATE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_update_filament_spool(void*, std::string, std::string, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_UPDATE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_delete_filament_spools(void*, BBL::FilamentDeleteParams, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_DELETE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_get_filament_config(void*, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_GET_FILAMENT_CONFIG_FAILED;
}

PANDAR_ABI int bambu_network_track_enable(void*, bool) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_remove_files(void*) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_event(void*, std::string, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_header(void*, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_update_property(void*, std::string, std::string, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_get_property(void*, std::string, std::string& value, std::string) {
    value.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

extern "C" {

struct ft_job_result {
    int ec;
    int resp_ec;
    const char* json;
    const void* bin;
    uint32_t bin_size;
};

struct ft_job_msg {
    int kind;
    const char* json;
};

typedef enum {
    FT_OK = 0,
    FT_EINVAL = -1,
    FT_ESTATE = -2,
    FT_EIO = -3,
    FT_ETIMEOUT = -4,
    FT_ECANCELLED = -5,
    FT_EXCEPTION = -6,
    FT_EUNKNOWN = -128
} ft_err;

using ft_tunnel_connect_cb = void (*)(void* user, int ok, int err, const char* msg);
using ft_tunnel_status_cb = void (*)(void* user, int old_status, int new_status, int err, const char* msg);
using ft_job_result_cb = void (*)(void* user, ft_job_result result);
using ft_job_msg_cb = void (*)(void* user, ft_job_msg msg);

struct FT_TunnelHandle;
struct FT_JobHandle;

}

namespace {

struct Tunnel {
    std::atomic<int> refs{1};
    ft_tunnel_status_cb status_cb = nullptr;
    void* status_user = nullptr;
    bool closed = false;
};

struct Job {
    std::atomic<int> refs{1};
    ft_job_result_cb result_cb = nullptr;
    void* result_user = nullptr;
    ft_job_msg_cb msg_cb = nullptr;
    void* msg_user = nullptr;
    bool cancelled = false;
    bool finished = false;
    ft_job_result result{};
    std::mutex mutex;
    std::condition_variable cv;
};

void retain(Tunnel* tunnel) {
    if (tunnel) tunnel->refs.fetch_add(1, std::memory_order_relaxed);
}

void release(Tunnel* tunnel) {
    if (tunnel && tunnel->refs.fetch_sub(1, std::memory_order_acq_rel) == 1) delete tunnel;
}

void retain(Job* job) {
    if (job) job->refs.fetch_add(1, std::memory_order_relaxed);
}

void release(Job* job) {
    if (job && job->refs.fetch_sub(1, std::memory_order_acq_rel) == 1) delete job;
}

}

PANDAR_ABI int ft_abi_version() { return 1; }
PANDAR_ABI void ft_free(void*) {}
PANDAR_ABI void ft_job_result_destroy(ft_job_result*) {}
PANDAR_ABI void ft_job_msg_destroy(ft_job_msg*) {}

PANDAR_ABI ft_err ft_tunnel_create(const char*, FT_TunnelHandle** out) {
    if (!out) return FT_EINVAL;
    *out = reinterpret_cast<FT_TunnelHandle*>(new Tunnel());
    return FT_OK;
}

PANDAR_ABI void ft_tunnel_retain(FT_TunnelHandle* h) { retain(reinterpret_cast<Tunnel*>(h)); }
PANDAR_ABI void ft_tunnel_release(FT_TunnelHandle* h) { release(reinterpret_cast<Tunnel*>(h)); }

PANDAR_ABI ft_err ft_tunnel_start_connect(FT_TunnelHandle* h, ft_tunnel_connect_cb cb, void* user) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    if (cb) cb(user, 1, FT_EIO, R"({"error":"unsupported_file_transfer"})");
    if (tunnel->status_cb) tunnel->status_cb(tunnel->status_user, 0, -1, FT_EIO, R"({"error":"unsupported_file_transfer"})");
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_sync_connect(FT_TunnelHandle* h) {
    return h ? FT_EIO : FT_EINVAL;
}

PANDAR_ABI ft_err ft_tunnel_set_status_cb(FT_TunnelHandle* h, ft_tunnel_status_cb cb, void* user) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    tunnel->status_cb = cb;
    tunnel->status_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_shutdown(FT_TunnelHandle* h) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    tunnel->closed = true;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_create(const char*, FT_JobHandle** out) {
    if (!out) return FT_EINVAL;
    *out = reinterpret_cast<FT_JobHandle*>(new Job());
    return FT_OK;
}

PANDAR_ABI void ft_job_retain(FT_JobHandle* h) { retain(reinterpret_cast<Job*>(h)); }
PANDAR_ABI void ft_job_release(FT_JobHandle* h) { release(reinterpret_cast<Job*>(h)); }

PANDAR_ABI ft_err ft_job_set_result_cb(FT_JobHandle* h, ft_job_result_cb cb, void* user) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->result_cb = cb;
    job->result_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_get_result(FT_JobHandle* h, uint32_t timeout_ms, ft_job_result* out) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job || !out) return FT_EINVAL;
    std::unique_lock<std::mutex> lock(job->mutex);
    if (!job->finished) {
        job->cv.wait_for(lock, std::chrono::milliseconds(timeout_ms), [job] { return job->finished; });
    }
    *out = job->finished ? job->result : ft_job_result{FT_ETIMEOUT, 0, nullptr, nullptr, 0};
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_start_job(FT_TunnelHandle* th, FT_JobHandle* jh) {
    if (!th || !jh) return FT_EINVAL;
    auto* job = reinterpret_cast<Job*>(jh);
    {
        std::lock_guard<std::mutex> lock(job->mutex);
        job->result = ft_job_result{FT_EIO, 0, nullptr, nullptr, 0};
        job->finished = true;
    }
    job->cv.notify_all();
    if (job->result_cb) job->result_cb(job->result_user, job->result);
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_cancel(FT_JobHandle* h) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->cancelled = true;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_set_msg_cb(FT_JobHandle* h, ft_job_msg_cb cb, void* user) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->msg_cb = cb;
    job->msg_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_try_get_msg(FT_JobHandle* h, ft_job_msg* out) {
    if (out) *out = ft_job_msg{};
    return h ? FT_EIO : FT_EINVAL;
}

PANDAR_ABI ft_err ft_job_get_msg(FT_JobHandle* h, uint32_t, ft_job_msg* out) {
    if (out) *out = ft_job_msg{};
    return h ? FT_EIO : FT_EINVAL;
}
