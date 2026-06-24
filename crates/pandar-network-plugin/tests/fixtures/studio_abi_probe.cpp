#include <cstdint>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace BBL {

using OnUpdateStatusFn = std::function<void(int, int, std::string)>;
using WasCancelledFn = std::function<bool()>;
using OnWaitFn = std::function<bool(int, std::string)>;

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

std::string frontend_url() {
    const char* value = std::getenv("PANDAR_PLUGIN_FRONTEND_URL");
    if (!value || value[0] == '\0') return {};
    std::string url(value);
    if (!url.empty() && url.back() != '/') url.push_back('/');
    return url;
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
    int ft_abi_version = 0;
    int ft_start_connect_rc = 0;
    int ft_sync_rc = 0;
    int ft_start_job_rc = 0;
    int ft_job_result_ec = 0;
    int ft_cancel_rc = 0;
    std::string update_body;
    std::string sdcard_update_body;
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
        << ",\"ft_abi_version\":" << result.ft_abi_version
        << ",\"ft_start_connect_rc\":" << result.ft_start_connect_rc
        << ",\"ft_sync_rc\":" << result.ft_sync_rc
        << ",\"ft_start_job_rc\":" << result.ft_start_job_rc
        << ",\"ft_job_result_ec\":" << result.ft_job_result_ec
        << ",\"ft_cancel_rc\":" << result.ft_cancel_rc
        << ",\"update_body\":" << escape_json(result.update_body)
        << ",\"sdcard_update_body\":" << escape_json(result.sdcard_update_body)
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
    using token_fn = int (*)(void*, std::string, unsigned int*, std::string*);
    using change_user_fn = int (*)(void*, std::string);
    using print_info_fn = int (*)(void*, unsigned int*, std::string*);
    using tasks_fn = int (*)(void*, BBL::TaskQueryParams, std::string*);
    using start_print_fn = int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn);
    using start_sdcard_fn = int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn);
    using connect_printer_fn = int (*)(void*, std::string, std::string, std::string, std::string, bool);
    using send_printer_fn = int (*)(void*, std::string, std::string, int, int);
    using logout_fn = int (*)(void*, bool);

    auto create_agent = lib.sym<create_agent_fn>("bambu_network_create_agent");
    auto destroy_agent = lib.sym<destroy_agent_fn>("bambu_network_destroy_agent");
    auto get_host = lib.sym<string_agent_fn>("bambu_network_get_bambulab_host");
    auto get_token = lib.sym<token_fn>("bambu_network_get_my_token");
    auto get_profile = lib.sym<token_fn>("bambu_network_get_my_profile");
    auto change_user = lib.sym<change_user_fn>("bambu_network_change_user");
    auto build_login_cmd = lib.sym<string_agent_fn>("bambu_network_build_login_cmd");
    auto build_login_info = lib.sym<string_agent_fn>("bambu_network_build_login_info");
    auto get_print_info = lib.sym<print_info_fn>("bambu_network_get_user_print_info");
    auto get_tasks = lib.sym<tasks_fn>("bambu_network_get_user_tasks");
    auto start_print = lib.sym<start_print_fn>("bambu_network_start_print");
    auto start_sdcard_print = lib.sym<start_sdcard_fn>("bambu_network_start_send_gcode_to_sdcard");
    auto connect_printer = lib.sym<connect_printer_fn>("bambu_network_connect_printer");
    auto send_printer = lib.sym<send_printer_fn>("bambu_network_send_message_to_printer");
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

    out.host = get_host(agent);
    if (out.host != frontend_url()) fail(agent, destroy_agent, "frontend host did not match environment");

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
        std::string profile_body;
        int profile_rc = get_profile(agent, "probe-token", &http_code, &profile_body);
        if (profile_rc != 0 || http_code != 200) fail(agent, destroy_agent, "profile retrieval failed");
        if (!contains(profile_body, "probe-user") || !contains(profile_body, "Probe User")) {
            fail(agent, destroy_agent, "profile retrieval did not return stored profile content");
        }
        if (change_user(agent, profile_body) != 0) fail(agent, destroy_agent, "change_user failed");
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

    out.direct_connect_rc = connect_printer(agent, "printer-1", "127.0.0.1", "user", "pass", false);
    out.direct_message_rc = send_printer(agent, "printer-1", "G28 X", 0, 0);
    if (out.direct_connect_rc == 0) {
        fail(agent, destroy_agent, "direct printer connect unexpectedly succeeded");
    }
    if (failure_mode) {
        if (out.direct_message_rc == 0) {
            fail(agent, destroy_agent, "direct printer message unexpectedly succeeded in failure mode");
        }
    } else if (out.direct_message_rc != 0) {
        fail(agent, destroy_agent, "direct printer message did not submit operation");
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
