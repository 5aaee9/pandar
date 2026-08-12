#include <atomic>
#include <cstdint>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <map>
#include <memory>
#include <string>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace BBL {
using OnUserLoginFn = std::function<void(int, bool)>;
using OnHttpErrorFn = std::function<void(unsigned, std::string)>;
using GetCountryCodeFn = std::function<std::string()>;
using GetSubscribeFailureFn = std::function<void(std::string)>;
using OnMsgArrivedFn = std::function<void(std::string)>;
using QueueOnMainFn = std::function<void(std::function<void()>)>;
using OnServerErrFn = std::function<void(std::string, int)>;
using OnMessageFn = std::function<void(std::string, std::string)>;
using OnUpdateStatusFn = std::function<void(int, int, std::string)>;
using WasCancelledFn = std::function<bool()>;
using OnWaitFn = std::function<bool(int, std::string)>;
using ProgressFn = std::function<void(int)>;

struct PrintParams {
    std::string dev_id, task_name, project_name, preset_name, filename, config_filename;
    int plate_index = 0;
    std::string ftp_folder, ftp_file, ftp_file_md5, nozzle_mapping, ams_mapping;
    std::string ams_mapping2, ams_mapping_info, nozzles_info, connection_type, comments;
    int origin_profile_id = 0;
    int stl_design_id = 0;
    std::string origin_model_id, print_type, dst_file, dev_name, dev_ip;
    bool use_ssl_for_ftp = false;
    bool use_ssl_for_mqtt = false;
    std::string username, password;
    bool task_bed_leveling = false, task_flow_cali = false, task_vibration_cali = false;
    bool task_layer_inspect = false, task_record_timelapse = false;
    bool task_timelapse_use_internal = false, task_use_ams = false;
    std::string task_bed_type, extra_options;
    int auto_bed_leveling = 0, auto_flow_cali = 0, auto_offset_cali = 0;
    int extruder_cali_manual_mode = -1;
    bool task_ext_change_assist = false, try_emmc_print = false;
    std::string svc_context;
};
}

namespace {
constexpr int kSuccess = 0;
constexpr int kInvalid = -19;
constexpr int kErrorStage = 7;

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

template<class Callback, class Setter>
std::weak_ptr<int> register_owned(void* agent, Setter setter, Callback callback) {
    auto owner = std::make_shared<int>(1);
    std::weak_ptr<int> weak = owner;
    Callback wrapped = [owner, callback](auto&&... args) mutable -> decltype(auto) {
        return callback(std::forward<decltype(args)>(args)...);
    };
    if (setter(agent, std::move(wrapped)) != kSuccess) fail("callback registration failed");
    owner.reset();
    if (weak.expired()) fail("callback was discarded instead of stored");
    return weak;
}
}

int main(int argc, char** argv) {
    if (argc != 3) fail("usage: disposition-probe <plugin> <config-dir>");
    Library library(argv[1]);
    if (!library.handle) fail("failed to load plugin");

    auto create = required<void* (*)(std::string)>(library, "bambu_network_create_agent");
    auto destroy = required<int (*)(void*)>(library, "bambu_network_destroy_agent");
    auto set_config = required<int (*)(void*, std::string)>(library, "bambu_network_set_config_dir");
    auto debug_consistent = required<bool (*)(bool)>(library, "bambu_network_check_debug_consistent");
    if (!debug_consistent(false) || debug_consistent(true)) {
        fail("release ABI did not reject debug Studio STL mode");
    }
    void* agent = create("");
    if (!agent || set_config(agent, argv[2]) != kSuccess) fail("agent setup failed");

    std::vector<std::function<bool()>> released;
    std::atomic<int> login_events{0}, http_events{0}, unexpected_events{0};
#define OWN(name, type, lambda) do { \
    auto setter = required<int (*)(void*, BBL::type)>(library, name); \
    auto weak = register_owned<BBL::type>(agent, setter, lambda); \
    released.push_back([weak] { return weak.expired(); }); \
} while (false)
    OWN("bambu_network_set_on_ssdp_msg_fn", OnMsgArrivedFn, [&](std::string) { ++unexpected_events; });
    OWN("bambu_network_set_on_user_login_fn", OnUserLoginFn, [&](int, bool) { ++login_events; });
    OWN("bambu_network_set_on_http_error_fn", OnHttpErrorFn, [&](unsigned, std::string) { ++http_events; });
    OWN("bambu_network_set_get_country_code_fn", GetCountryCodeFn, [&] { ++unexpected_events; return "US"; });
    OWN("bambu_network_set_on_subscribe_failure_fn", GetSubscribeFailureFn, [&](std::string) { ++unexpected_events; });
    OWN("bambu_network_set_on_user_message_fn", OnMessageFn, [&](std::string, std::string) { ++unexpected_events; });
    OWN("bambu_network_set_queue_on_main_fn", QueueOnMainFn, [&](std::function<void()>) { ++unexpected_events; });
    OWN("bambu_network_set_server_callback", OnServerErrFn, [&](std::string, int) { ++unexpected_events; });
#undef OWN

    auto change_user = required<int (*)(void*, std::string)>(library, "bambu_network_change_user");
    auto logout = required<int (*)(void*, bool)>(library, "bambu_network_user_logout");
    if (change_user(agent, R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant","tenant_name":"Tenant"})") != kSuccess ||
        logout(agent, false) != kSuccess || login_events.load() != 2) {
        fail("typed login/logout did not drive exactly two user callbacks");
    }
    unsigned http_code = 0;
    std::string http_body;
    auto get_token = required<int (*)(void*, std::string, unsigned*, std::string*)>(library, "bambu_network_get_my_token");
    if (get_token(agent, "", &http_code, &http_body) != kInvalid || http_code != 401 ||
        http_events.load() != 1) fail("typed ticket failure did not drive one HTTP callback");

    auto unsupported0 = [&](const char* name) {
        if (required<int (*)(void*)>(library, name)(agent) != kInvalid) fail(std::string(name) + " was not explicit -19");
    };
    unsupported0("bambu_network_init_log");
    unsupported0("bambu_network_update_cert");
    auto cert = required<int (*)(void*, std::string, std::string)>(library, "bambu_network_set_cert_file");
    if (cert(agent, "folder", "cert.pem") != kInvalid) fail("certificate configuration was not explicit -19");
    auto start_sub = required<int (*)(void*, std::string)>(library, "bambu_network_start_subscribe");
    auto stop_sub = required<int (*)(void*, std::string)>(library, "bambu_network_stop_subscribe");
    if (start_sub(agent, "app") != kInvalid || stop_sub(agent, "app") != kInvalid) fail("start/stop subscribe silently succeeded");
    auto consent = required<int (*)(void*, std::string)>(library, "bambu_network_report_consent");
    if (consent(agent, "accepted") != kInvalid) fail("unstored consent silently succeeded");
    auto send = required<int (*)(void*, std::string, std::string, int, int)>(
        library, "bambu_network_send_message");
    if (send(agent, " ", R"({"upgrade":{"command":"upgrade_confirm","sequence_id":"missing"}})", 0, 0) != kInvalid) {
        fail("firmware identity admission bypassed the Rust boundary");
    }

    int stage = 0, code = 0, updates = 0;
    std::string update_body;
    auto local_record = required<int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn)>(
        library, "bambu_network_start_local_print_with_record");
    if (local_record(agent, {}, [&](int s, int c, std::string body) { ++updates; stage=s; code=c; update_body=std::move(body); }, [] { return false; }, {}) != kInvalid ||
        updates != 1 || stage != kErrorStage || code != kInvalid ||
        update_body.find("unsupported_local_print_with_record") == std::string::npos ||
        update_body.find("\"disposition_version\":1") == std::string::npos) {
        fail("local print with record did not report one explicit unsupported error");
    }

    auto settings = required<int (*)(void*, std::string, BBL::ProgressFn, BBL::WasCancelledFn)>(library, "bambu_network_get_setting_list");
    int progress = 0;
    if (settings(agent, "", [&](int) { ++progress; }, [] { return false; }) != -9 || progress != 0) fail("settings did not fail explicitly without an authenticated account");
    auto extra = required<int (*)(void*, std::map<std::string,std::string>)>(library, "bambu_network_set_extra_http_header");
    if (extra(agent, {}) != kInvalid) fail("extra headers silently succeeded");
    auto task_report = required<int (*)(void*, int*, bool*)>(library, "bambu_network_check_user_task_report");
    int task_id = 9; bool printable = true;
    if (task_report(agent, &task_id, &printable) != kInvalid) fail("task report silently succeeded");

    auto hms = required<int (*)(void*, std::string&, std::string&, std::function<void(std::string,int)>)>(library, "bambu_network_get_hms_snapshot");
    std::string hms_a, hms_b; int hms_callbacks = 0;
    if (hms(agent, hms_a, hms_b, [&](std::string, int) { ++hms_callbacks; }) != kInvalid || hms_callbacks != 0) fail("HMS silently succeeded or invoked callback");
    auto staff = required<int (*)(void*, int, int, std::function<void(std::string)>)>(library, "bambu_network_get_design_staffpick");
    int maker_callbacks = 0;
    if (staff(agent, 0, 10, [&](std::string) { ++maker_callbacks; }) != kInvalid || maker_callbacks != 0) fail("MakerWorld silently succeeded or invoked callback");

    auto track = required<int (*)(void*, std::string, std::string)>(library, "bambu_network_track_event");
    if (track(agent, "category", "event") != kSuccess) fail("never-track policy was not a benign no-op");
    if (unexpected_events.load() != 0) fail("registered-only callbacks were invoked spuriously");

    if (destroy(agent) != kSuccess) fail("destroy failed");
    for (const auto& is_released : released) if (!is_released()) fail("destroy did not clear a registered callback");
    std::cout << R"({"ok":true,"version":1})" << '\n';
}
