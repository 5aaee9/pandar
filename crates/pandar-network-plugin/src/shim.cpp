#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
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
PluginHttpResult pandar_plugin_get_printers(const uint8_t*, std::size_t, const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_get_jobs(const uint8_t*, std::size_t, const uint8_t*, std::size_t);
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
    const uint8_t*, std::size_t
);
void pandar_plugin_free(void*, std::size_t);
void pandar_plugin_free_with_capacity(void*, std::size_t, std::size_t);
}

struct Agent {
    explicit Agent(std::string log_dir_value) : log_dir(std::move(log_dir_value)) {}

    std::string log_dir;
    std::string config_dir;
    std::string cert_folder;
    std::string cert_filename;
    std::string country_code;
    std::string selected_machine;
    std::string token;
    std::string user_id;
    std::string user_name;
    std::string avatar;
    std::string profile_json;
    std::string hub_url = "http://localhost:8080";
    std::string frontend_url = "http://localhost:3000/";
    std::string last_error;
    bool connected = false;
};

Agent* as_agent(void* raw) {
    return reinterpret_cast<Agent*>(raw);
}

bool has_hub(const Agent* agent) {
    return agent && !agent->hub_url.empty();
}

std::string env_or(const char* name, std::string fallback) {
    if (const char* value = std::getenv(name); value && value[0] != '\0') {
        return value;
    }
    return fallback;
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
    const auto quote = json.find('"', colon + 1);
    if (quote == std::string::npos) return {};
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
    if (const auto v = field_from_json(json, "user_id"); !v.empty()) agent->user_id = v;
    if (const auto v = field_from_json(json, "user_name"); !v.empty()) agent->user_name = v;
    if (const auto v = field_from_json(json, "tenant_name"); !v.empty() && agent->user_name.empty()) agent->user_name = v;
}

std::string body_from_result(PluginHttpResult result) {
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
        pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    }
    return body;
}

PluginHttpResult rust_exchange_ticket(const Agent* agent, const std::string& ticket) {
    return pandar_plugin_exchange_ticket(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(ticket.data()),
        ticket.size()
    );
}

PluginHttpResult rust_get_printers(const Agent* agent) {
    return pandar_plugin_get_printers(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
}

PluginHttpResult rust_get_jobs(const Agent* agent) {
    return pandar_plugin_get_jobs(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
}

PluginHttpResult rust_submit_print(const Agent* agent, const BBL::PrintParams& params) {
    return pandar_plugin_submit_print(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        reinterpret_cast<const uint8_t*>(params.dev_id.data()),
        params.dev_id.size(),
        reinterpret_cast<const uint8_t*>(params.task_name.empty() ? params.project_name.data() : params.task_name.data()),
        params.task_name.empty() ? params.project_name.size() : params.task_name.size(),
        reinterpret_cast<const uint8_t*>(params.filename.data()),
        params.filename.size(),
        params.plate_index,
        params.task_use_ams,
        params.task_flow_cali,
        params.task_record_timelapse,
        reinterpret_cast<const uint8_t*>(params.ams_mapping.data()),
        params.ams_mapping.size(),
        reinterpret_cast<const uint8_t*>(params.ams_mapping2.data()),
        params.ams_mapping2.size()
    );
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
    return a ? a->user_id : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_name(void* agent) {
    auto* a = as_agent(agent);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_avatar(void* agent) {
    auto* a = as_agent(agent);
    return a ? a->avatar : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_nickanme(void* agent) {
    auto* a = as_agent(agent);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_build_login_cmd(void* agent) {
    return login_envelope(as_agent(agent), false);
}

PANDAR_ABI std::string bambu_network_build_logout_cmd(void* agent) {
    return login_envelope(as_agent(agent), true);
}

PANDAR_ABI std::string bambu_network_build_login_info(void* agent) {
    return login_envelope(as_agent(agent), false);
}

PANDAR_ABI std::string bambu_network_get_bambulab_host(void* agent) {
    auto* a = as_agent(agent);
    return a ? a->frontend_url : env_or("PANDAR_PLUGIN_FRONTEND_URL", "http://localhost:3000/");
}

PANDAR_ABI std::string bambu_network_get_user_selected_machine(void* agent) {
    auto* a = as_agent(agent);
    return a ? a->selected_machine : std::string{};
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
    agent->hub_url = env_or("PANDAR_PLUGIN_HUB_URL", env_or("APP_API_URL", "http://localhost:8080"));
    agent->frontend_url = env_or("PANDAR_PLUGIN_FRONTEND_URL", env_or("APP_BASE_URL", "http://localhost:3000/"));
    if (!agent->frontend_url.empty() && agent->frontend_url.back() != '/') {
        agent->frontend_url.push_back('/');
    }
    return agent;
}

PANDAR_ABI int bambu_network_destroy_agent(void* agent) {
    delete as_agent(agent);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_init_log(void*) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_config_dir(void* agent, std::string config_dir) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->config_dir = std::move(config_dir);
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

PANDAR_ABI int bambu_network_start(void*) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

#define PANDAR_CALLBACK_SETTER(name, type) \
    PANDAR_ABI int name(void* agent, BBL::type) { \
        return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE; \
    }

PANDAR_CALLBACK_SETTER(bambu_network_set_on_ssdp_msg_fn, OnMsgArrivedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_login_fn, OnUserLoginFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_printer_connected_fn, OnPrinterConnectedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_server_connected_fn, OnServerConnectedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_http_error_fn, OnHttpErrorFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_get_country_code_fn, GetCountryCodeFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_subscribe_failure_fn, GetSubscribeFailureFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_message_fn, OnMessageFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_message_fn, OnMessageFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_local_connect_fn, OnLocalConnectedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_local_message_fn, OnMessageFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_queue_on_main_fn, QueueOnMainFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_server_callback, OnServerErrFn)

#undef PANDAR_CALLBACK_SETTER

PANDAR_ABI int bambu_network_connect_server(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->connected = has_hub(a);
    if (a->connected) a->last_error.clear();
    return a->connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
}

PANDAR_ABI bool bambu_network_is_server_connected(void* agent) {
    auto* a = as_agent(agent);
    return a && a->connected && has_hub(a) && a->last_error.empty();
}

PANDAR_ABI int bambu_network_refresh_connection(void* agent) {
    return bambu_network_connect_server(agent);
}

PANDAR_ABI int bambu_network_start_subscribe(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_stop_subscribe(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_add_subscribe(void* agent, std::vector<std::string>) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_del_subscribe(void* agent, std::vector<std::string>) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI void bambu_network_enable_multi_machine(void*, bool) {}

PANDAR_ABI int bambu_network_send_message(void* agent, std::string, std::string, int, int) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_connect_printer(void* agent, std::string, std::string, std::string, std::string, bool) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_disconnect_printer(void* agent) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_send_message_to_printer(void* agent, std::string, std::string, int, int) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_update_cert(void*) {
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI void bambu_network_install_device_cert(void*, std::string, bool) {}

PANDAR_ABI bool bambu_network_start_discovery(void*, bool, bool) {
    return false;
}

PANDAR_ABI int bambu_network_change_user(void* agent, std::string user_info) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (user_info.empty() || user_info == "{}") {
        a->token.clear();
        a->profile_json.clear();
        a->user_id.clear();
        a->user_name.clear();
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    apply_profile_json(a, user_info);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI bool bambu_network_is_user_login(void* agent) {
    auto* a = as_agent(agent);
    return a && !a->token.empty();
}

PANDAR_ABI int bambu_network_user_logout(void* agent, bool) {
    auto* a = as_agent(agent);
    if (a) {
        a->token.clear();
        a->profile_json.clear();
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_profile(void* agent, std::string token, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (!token.empty()) a->token = std::move(token);
    if (a->profile_json.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"profile_unavailable"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    success_body(http_code, http_body, profile_body(a));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_token(void* agent, std::string ticket, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
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
    a->token = field_from_json(body, "token");
    a->profile_json = object_from_json(body, "profile");
    apply_profile_json(a, a->profile_json);
    a->last_error.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_info(void* agent, int* identifier) {
    if (identifier) *identifier = as_agent(agent) ? 1 : 0;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_user_selected_machine(void* agent, std::string dev_id) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
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
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    if (a->token.empty() || params.dev_id.empty() || params.filename.empty()) {
        if (update_fn) update_fn(7, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, "Pandar plugin print submission is missing token, printer, or artifact");
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_submit_print(a, params);
    std::string body = body_from_result(result);
    if (result.status != 0) {
        a->last_error = body;
        if (update_fn) update_fn(7, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, body);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    if (update_fn) update_fn(100, BBL::BAMBU_NETWORK_SUCCESS, body);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_local_print_with_record(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    return bambu_network_start_print(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), std::move(wait_fn));
}

PANDAR_ABI int bambu_network_start_send_gcode_to_sdcard(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    if (!as_agent(agent)) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (update_fn) update_fn(7, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, "Pandar plugin SD-card print is unsupported");
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    (void)params;
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_start_local_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
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
    if (a->token.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    }
    auto result = rust_get_printers(a);
    if (http_code) *http_code = result.http_code;
    if (http_body) *http_body = body_from_result(result);
    if (result.status != 0) return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_tasks(void* agent, BBL::TaskQueryParams, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (a->token.empty()) {
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_get_jobs(a);
    if (http_body) *http_body = body_from_result(result);
    if (result.status != 0) return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
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

PANDAR_ABI int bambu_network_get_camera_url(void*, std::string, std::function<void(std::string)> callback) {
    if (callback) callback({});
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_camera_url_for_golive(void*, std::string, std::string, std::function<void(std::string)> callback) {
    if (callback) callback({});
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
    if (cb) cb(user, 1, FT_EIO, "Pandar plugin does not open direct file-transfer tunnels");
    if (tunnel->status_cb) tunnel->status_cb(tunnel->status_user, 0, -1, FT_EIO, "unsupported");
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
