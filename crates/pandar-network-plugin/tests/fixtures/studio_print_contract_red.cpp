#include <cstdint>
#include <cstdlib>
#include <chrono>
#include <filesystem>
#include <functional>
#include <iostream>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include "bambu_networking.hpp"
#include "studio_model_task_consumer.hpp"
#include "studio_print_consumer.hpp"
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace {

[[noreturn]] void fail(const std::string& message)
{
    std::cerr << message << '\n';
    std::exit(10);
}

class Library {
public:
    explicit Library(const char* path)
    {
#ifdef _WIN32
        handle_ = LoadLibraryA(path);
#else
        handle_ = dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
        if (!handle_) fail("failed to load plugin");
    }

    ~Library()
    {
#ifdef _WIN32
        FreeLibrary(handle_);
#else
        dlclose(handle_);
#endif
    }

    template<class Function> Function require(const char* name) const
    {
#ifdef _WIN32
        auto* symbol = reinterpret_cast<void*>(GetProcAddress(handle_, name));
#else
        auto* symbol = dlsym(handle_, name);
#endif
        if (!symbol) fail(std::string("missing plugin symbol: ") + name);
        return reinterpret_cast<Function>(symbol);
    }

private:
#ifdef _WIN32
    HMODULE handle_{};
#else
    void* handle_{};
#endif
};

std::string json_escape(const std::string& value)
{
    std::string out;
    for (const char ch : value) {
        switch (ch) {
        case '\\': out += "\\\\"; break;
        case '"': out += "\\\""; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default: out += ch; break;
        }
    }
    return out;
}

void print_ints(const std::vector<int>& values)
{
    std::cout << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index) std::cout << ',';
        std::cout << values[index];
    }
    std::cout << ']';
}

void print_strings(const std::vector<std::string>& values)
{
    std::cout << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index) std::cout << ',';
        std::cout << '"' << json_escape(values[index]) << '"';
    }
    std::cout << ']';
}

struct Api {
    using Create = void* (*)(std::string);
    using Destroy = int (*)(void*);
    using IsLogin = bool (*)(void*);
    using SetConfigDir = int (*)(void*, std::string);
    using ChangeUser = int (*)(void*, std::string);
    using GetPrintInfo = int (*)(void*, unsigned int*, std::string*);
    using StartPrint = int (*)(void*, BBL::PrintParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, BBL::OnWaitFn);
    using GetTasks = int (*)(void*, BBL::TaskQueryParams, std::string*);
    using GetPlate = int (*)(void*, std::string, int*);
    using GetSubtask = int (*)(void*, std::string, std::string*, unsigned int*, std::string*);
    using GetSlice = int (*)(void*, std::string, std::string, int, std::string*);
    using GetModelSubtask = int (*)(void*, Slic3r::BBLModelTask*, std::function<void(Slic3r::BBLModelTask*)>);
    explicit Api(const Library& library)
        : create(library.require<Create>("bambu_network_create_agent"))
        , destroy(library.require<Destroy>("bambu_network_destroy_agent"))
        , start(library.require<Destroy>("bambu_network_start"))
        , is_login(library.require<IsLogin>("bambu_network_is_user_login"))
        , set_config_dir(library.require<SetConfigDir>("bambu_network_set_config_dir"))
        , change_user(library.require<ChangeUser>("bambu_network_change_user"))
        , get_print_info(library.require<GetPrintInfo>("bambu_network_get_user_print_info"))
        , start_print(library.require<StartPrint>("bambu_network_start_print"))
        , get_tasks(library.require<GetTasks>("bambu_network_get_user_tasks"))
        , get_plate(library.require<GetPlate>("bambu_network_get_task_plate_index"))
        , get_subtask(library.require<GetSubtask>("bambu_network_get_subtask_info"))
        , get_slice(library.require<GetSlice>("bambu_network_get_slice_info"))
        , get_model_subtask(library.require<GetModelSubtask>("bambu_network_get_subtask"))
    {}
    Create create;
    Destroy destroy;
    Destroy start;
    IsLogin is_login;
    SetConfigDir set_config_dir;
    ChangeUser change_user;
    GetPrintInfo get_print_info;
    StartPrint start_print;
    GetTasks get_tasks;
    GetPlate get_plate;
    GetSubtask get_subtask;
    GetSlice get_slice;
    GetModelSubtask get_model_subtask;
};

void* configured_agent(const Api& api, const std::string& config_dir, const std::string& name)
{
    void* agent = api.create("studio-print-contract-red");
    if (!agent) fail("agent creation failed");
    if (api.set_config_dir(agent, config_dir) != 0) fail("config directory setup failed");
    const std::string profile =
        R"({"token":"contract-token","user_id":"contract-user","user_name":"Contract User","tenant_id":"contract-tenant","tenant_name":"Contract Tenant"})";
    if (name != "model_task_destroy_no_auth_recovery" && api.change_user(agent, profile) != 0) fail("profile setup failed");
    if (name == "trailing_slash_hub" && (api.start(agent) != 0 || !api.is_login(agent)))
        fail("trailing slash startup discarded the persisted login");
    unsigned int http_code = 0;
    std::string body;
    if (api.get_print_info(agent, &http_code, &body) != 0 || http_code != 200) {
        fail("printer cache seed failed");
    }
    return agent;
}
void apply_case(BBL::PrintParams& params, const std::string& name, const std::string& config_file)
{
    if (name == "task_name") params.task_name = "task-sentinel.3mf";
    else if (name == "project_name") params.project_name = "project-sentinel";
    else if (name == "preset_name") params.preset_name = "preset-sentinel";
    else if (name == "config_filename" || name == "cancel_before_create" || name == "invalid_config_xml") params.config_filename = config_file;
    else if (name == "plate_index") params.plate_index = 713;
    else if (name == "invalid_plate_index") params.plate_index = 0;
    else if (name == "ftp_folder") params.ftp_folder = "/contract/private/ftp-folder";
    else if (name == "ftp_file") params.ftp_file = "ftp-object.3mf";
    else if (name == "ftp_file_md5") params.ftp_file_md5 = "0123456789abcdef0123456789abcdef";
    else if (name == "nozzle_mapping") params.nozzle_mapping = "[1,0]";
    else if (name == "ams_mapping") params.ams_mapping = "[17,23]";
    else if (name == "ams_mapping2") params.ams_mapping2 = R"([{"ams_id":17,"slot_id":23}])";
    else if (name == "ams_mapping_info") params.ams_mapping_info = R"([{"ams":17,"targetColor":"11223344","filamentId":"GFA00","filamentType":"PLA","nozzleId":0,"sourceColor":"55667788"}])";
    else if (name == "nozzles_info") params.nozzles_info = R"([{"id":0,"type":null,"flowSize":"H","diameter":0.4},{"id":1,"type":null,"flowSize":"S","diameter":0.6}])";
    else if (name == "comments") params.comments = "comment-sentinel";
    else if (name == "origin_profile_id") params.origin_profile_id = 29;
    else if (name == "stl_design_id") params.stl_design_id = 31;
    else if (name == "origin_model_id") params.origin_model_id = "model-sentinel";
    else if (name == "dst_file") params.dst_file = "sdcard/contract.3mf";
    else if (name == "dev_name") params.dev_name = "device-name-sentinel";
    else if (name == "dev_ip") params.dev_ip = "198.51.100.77";
    else if (name == "use_ssl_for_ftp") params.use_ssl_for_ftp = true;
    else if (name == "use_ssl_for_mqtt") params.use_ssl_for_mqtt = true;
    else if (name == "username") params.username = "username-secret-sentinel";
    else if (name == "password") params.password = "password-secret-sentinel";
    else if (name == "task_bed_leveling") params.task_bed_leveling = true;
    else if (name == "task_flow_cali") params.task_flow_cali = true;
    else if (name == "task_vibration_cali") params.task_vibration_cali = true;
    else if (name == "task_layer_inspect") params.task_layer_inspect = true;
    else if (name == "task_record_timelapse") params.task_record_timelapse = true;
    else if (name == "task_timelapse_use_internal") params.task_timelapse_use_internal = true;
    else if (name == "task_use_ams") params.task_use_ams = true;
    else if (name == "task_bed_type") params.task_bed_type = "supertack_plate";
    else if (name == "extra_options") params.extra_options = R"({"future":true})";
    else if (name == "auto_bed_leveling") params.auto_bed_leveling = 2;
    else if (name == "auto_flow_cali") params.auto_flow_cali = 2;
    else if (name == "auto_offset_cali") params.auto_offset_cali = 2;
    else if (name == "extruder_cali_manual_mode") params.extruder_cali_manual_mode = 1;
    else if (name == "task_ext_change_assist") params.task_ext_change_assist = true;
    else if (name == "try_emmc_print") params.try_emmc_print = true;
    else if (name == "svc_context") params.svc_context = "service-context-sentinel";
#if defined(PANDAR_STUDIO_PRINT_SLICER_UID)
    else if (name == "slicer_uid") params.slicer_uid = "slicer-uid-sentinel";
#endif
#if defined(PANDAR_STUDIO_PRINT_QUEUE_PLATE_ID)
    else if (name == "queue_plate_id") params.queue_plate_id = "queue-plate-sentinel";
#endif
    else if (name == "invalid_nozzle_mapping") params.nozzle_mapping = "{";
    else if (name == "invalid_ams_mapping") params.ams_mapping = "{";
    else if (name == "invalid_ams_mapping2") params.ams_mapping2 = "{";
    else if (name == "invalid_ams_mapping_info") params.ams_mapping_info = "{";
    else if (name == "invalid_nozzles_info") params.nozzles_info = "{";
    else if (name == "whitespace_nozzle_mapping") params.nozzle_mapping = " ";
    else if (name == "schema_nozzle_mapping") params.nozzle_mapping = R"(["/private/diagnostic-secret-token@198.51.100.91"])";
    else if (name == "schema_ams_mapping") params.ams_mapping = R"({"wrong":[]})";
    else if (name == "schema_ams_mapping2") params.ams_mapping2 = R"([{"ams_id":"17","slot_id":23}])";
    else if (name == "schema_ams_mapping_info") params.ams_mapping_info = R"([{"ams":17,"targetColor":1,"filamentId":"GFA00","filamentType":"PLA","nozzleId":0,"sourceColor":"55667788"}])";
    else if (name == "schema_nozzles_info") params.nozzles_info = R"([{"id":0,"type":null,"flowSize":"H","diameter":"0.4"}])";
    else if (name == "invalid_task_bed_type") params.task_bed_type = "unknown";
    else if (name == "empty_connection_type") params.connection_type = "";
    else if (name == "invalid_connection_type") params.connection_type = "telepathy";
    else if (name == "unsupported_print_type") params.print_type = "from_sdcard_view";
    else if (name == "invalid_auto_bed_leveling") params.auto_bed_leveling = 3;
    else if (name == "invalid_auto_flow_cali") params.auto_flow_cali = 3;
    else if (name == "invalid_auto_offset_cali") params.auto_offset_cali = 3;
    else if (name == "invalid_extruder_cali_manual_mode") params.extruder_cali_manual_mode = 2;
}
std::thread replace_account_during_request(
    const Api& api,
    void* agent,
    const std::string& name,
    const std::string& config_dir
)
{
    if (name != "stale_task_list" && name != "stale_task_plate"
        && name != "stale_task_subtask" && name != "stale_model_task"
        && name != "stale_during_detail") return {};
    return std::thread([&api, agent, config_dir] {
        const auto entered = std::filesystem::path(config_dir) / "request-entered";
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
        while (!std::filesystem::exists(entered) && std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (!std::filesystem::exists(entered)) fail("account freshness request did not enter Hub");
        if (api.change_user(agent, R"({"token":"replacement-token","user_id":"replacement-user"})") != 0) {
            fail("replacement account setup failed");
        }
        std::filesystem::create_directory(std::filesystem::path(config_dir) / "release-request");
    });
}
void run_print(
    const Api& api,
    void* agent,
    const std::string& name,
    const char* artifact,
    const char* config_dir,
    const char* config_file
)
{
    BBL::PrintParams params{};
    params.dev_id = "studio-serial-1";
    params.task_name = "contract-base.3mf";
    params.project_name = "contract-base-project";
    params.filename = artifact;
    params.plate_index = 1;
    params.connection_type = "cloud";
    params.print_type = "from_normal";
    params.task_bed_type = "textured_plate";
    apply_case(params, name, config_file);
    std::vector<int> stages;
    std::vector<int> codes;
    std::vector<std::string> bodies;
    int wait_count = 0;
    int cancel_check_count = 0;
    int wait_state = -999;
    std::string wait_info;
    const auto update = [&](int stage, int code, std::string body) {
        stages.push_back(stage);
        codes.push_back(code);
        bodies.push_back(std::move(body));
        if (name == "stale_after_201" && stage == BBL::PrintingStageWaiting) {
            api.change_user(
                agent,
                R"({"token":"replacement-token","user_id":"replacement-user"})"
            );
        }
        if (name == "stale_cancel_failed" && stage == BBL::PrintingStageSending) {
            api.change_user(
                agent,
                R"({"token":"replacement-token","user_id":"replacement-user"})"
            );
        }
    };
    const auto cancelled = [&] {
        if (name == "cancel_before_create") return ++cancel_check_count >= 2;
        if (name == "cancel") return true;
        if (stages.empty()) return false;
        if (name == "cancel_upload") return stages.back() == BBL::PrintingStageUpload;
        if (name == "cancel_queued" || name == "cancel_wrong_id") {
            return stages.back() == BBL::PrintingStageWaiting;
        }
        if (name == "cancel_race_stale" && stages.back() == BBL::PrintingStageWaiting) {
            api.change_user(
                agent,
                R"({"token":"replacement-token","user_id":"replacement-user"})"
            );
            return true;
        }
        if (name == "cancel_too_late") return stages.back() == BBL::PrintingStageSending;
        if (name == "stale_cancel_failed") return stages.back() == BBL::PrintingStageSending;
        if (name == "cancel_after_wait" || name == "cancel_during_failed_wait") return wait_count > 0;
        if (name == "cancel_at_stage_five") return stages.back() == 5;
        return false;
    };
    const auto wait = [&](int state, std::string info) {
        ++wait_count;
        wait_state = state;
        wait_info = std::move(info);
        return name != "wait_false" && name != "cancel_during_failed_wait";
    };
    auto account_replacement = replace_account_during_request(api, agent, name, config_dir);
    const int rc = api.start_print(agent, std::move(params), update, cancelled, wait);
    if (account_replacement.joinable()) account_replacement.join();
    std::cout << "{\"rc\":" << rc << ",\"stages\":";
    print_ints(stages);
    std::cout << ",\"codes\":";
    print_ints(codes);
    std::cout << ",\"bodies\":";
    print_strings(bodies);
    std::cout << ",\"wait_count\":" << wait_count << ",\"wait_state\":" << wait_state
              << ",\"wait_info\":\"" << json_escape(wait_info) << "\"}\n";
}
void run_tasks(const Api& api, void* agent, const std::string& name, const char* config_dir)
{
    BBL::TaskQueryParams query;
    query.dev_id = "studio-serial-1";
    query.status = 1;
    query.offset = 0;
    query.limit = 5;
    auto account_replacement = replace_account_during_request(api, agent, name, config_dir);
    std::string tasks_body;
    const int tasks_rc = api.get_tasks(agent, query, &tasks_body);
    int plate_index = -1;
    const std::string task_id = name == "task_unknown" ? "99999" : "38191";
    const int plate_rc = api.get_plate(agent, task_id, &plate_index);
    std::string subtask_json;
    std::string subtask_http_body;
    unsigned int subtask_http_code = 0;
    const int subtask_rc = api.get_subtask(
        agent, task_id, &subtask_json, &subtask_http_code, &subtask_http_body
    );
    if (account_replacement.joinable() && name != "stale_model_task") account_replacement.join();
    const bool task_consumer_ok = studio_print_consumer::consume_task_page(tasks_body);
    const bool subtask_consumer_ok = studio_print_consumer::consume_subtask(subtask_json);
    std::string slice_json = "sentinel";
    const int slice_rc = api.get_slice(agent, "38191", "38191", 7, &slice_json);
    auto model_subtask = studio_model_task_consumer::invoke(
        [&](auto* task, auto callback) { return api.get_model_subtask(agent, task, callback); },
        task_id, name, config_dir, [&] { return api.change_user(agent, R"({"token":"replacement-token","user_id":"replacement-user"})"); }
    );
    if (account_replacement.joinable()) account_replacement.join();
    model_subtask.destroy_agent([&] { api.destroy(agent); });

    std::cout << "{\"tasks_rc\":" << tasks_rc << ",\"tasks_body\":\""
              << json_escape(tasks_body) << "\",\"plate_rc\":" << plate_rc
              << ",\"plate_index\":" << plate_index << ",\"subtask_rc\":" << subtask_rc
              << ",\"subtask_http_code\":" << subtask_http_code
              << ",\"subtask_json\":\"" << json_escape(subtask_json)
              << "\",\"subtask_http_body\":\"" << json_escape(subtask_http_body)
              << "\",\"task_consumer_ok\":" << (task_consumer_ok ? "true" : "false")
              << ",\"subtask_consumer_ok\":" << (subtask_consumer_ok ? "true" : "false")
              << ",\"task_consumer_hash\":\"" PANDAR_TASK_CONSUMER_HASH "\""
              << ",\"subtask_consumer_hash\":\"" PANDAR_SUBTASK_CONSUMER_HASH "\""
              << ",\"slice_rc\":" << slice_rc << ",\"slice_json\":\""
              << json_escape(slice_json) << '"';
    model_subtask.write_json_fields(std::cout, json_escape);
    std::cout << "}\n";
}

} // namespace

int main(int argc, char** argv)
{
    if (argc != 7) {
        std::cerr << "usage: studio_print_contract_red <plugin> <print|tasks> <case> <artifact> <config-dir> <config-file>\n";
        return 2;
    }
    const Library library(argv[1]);
    const Api api(library);
    void* agent = configured_agent(api, argv[5], argv[3]);
    if (std::string(argv[2]) == "print") run_print(api, agent, argv[3], argv[4], argv[5], argv[6]);
    else if (std::string(argv[2]) == "tasks") {
        run_tasks(api, agent, argv[3], argv[5]);
        agent = nullptr;
    }
    else fail("unknown mode");
    if (agent) api.destroy(agent);
    return 0;
}
