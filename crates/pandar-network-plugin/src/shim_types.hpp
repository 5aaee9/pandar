#pragma once

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <deque>
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

#include "shim_ams_types.hpp"
#include "shim_lifetime_state.hpp"
#include "shim_model_task_types.hpp"

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
constexpr int BAMBU_NETWORK_ERR_AMS_SYNC_FAILED = -32;
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
#if defined(PANDAR_STUDIO_PRINT_SVC_CONTEXT)
    std::string svc_context;
#endif
#if defined(PANDAR_STUDIO_PRINT_SLICER_UID)
    std::string slicer_uid;
#endif
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

namespace pandar::network_plugin {

extern "C" {
struct PluginHttpResult {
    int32_t status;
    uint32_t http_code;
    uint8_t* body_ptr;
    std::size_t body_len;
    std::size_t body_cap;
};

struct PluginFirmwareCallbackResult {
    int32_t status;
    uint64_t generation;
    uint64_t origin_tick;
    uint64_t local_generation;
    uint64_t cache_generation;
    uint8_t* dev_id_ptr;
    std::size_t dev_id_len;
    std::size_t dev_id_cap;
    uint8_t* message_ptr;
    std::size_t message_len;
    std::size_t message_cap;
    int32_t tunnel;
};

struct PluginCoreFirmwareObservation {
    uint64_t generation;
    uint64_t sequence;
};

void* pandar_plugin_core_create(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
void pandar_plugin_core_destroy(void*);
void* pandar_plugin_core_connection_session(void*);
void* pandar_plugin_core_firmware_session(void*);
uint64_t pandar_plugin_core_account_identity(void*);
PluginCoreFirmwareObservation pandar_plugin_core_reserve_firmware_observation(void*);

const char* pandar_plugin_network_agent_version();
#if defined(PANDAR_STUDIO_AMS_SYNC)
PluginHttpResult pandar_plugin_sync_ams_filaments(bool);
#endif
PluginHttpResult pandar_plugin_camera_url(void*, const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_local_connect_json(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);

PluginHttpResult pandar_plugin_exchange_ticket(const uint8_t*, std::size_t, const uint8_t*, std::size_t);
uint64_t pandar_plugin_firmware_session_sync_account(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
uint64_t pandar_plugin_firmware_session_fence_account(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
uint64_t pandar_plugin_firmware_session_generation(void*);
int32_t pandar_plugin_firmware_session_generation_current(void*, uint64_t);

PluginHttpResult pandar_plugin_firmware_catalog(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    uint64_t
);
PluginHttpResult pandar_plugin_firmware_refresh_version(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    uint64_t
);
PluginHttpResult pandar_plugin_firmware_send(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    int32_t,
    uint64_t*,
    uint64_t
);
int32_t pandar_plugin_firmware_return_handoff(
    void*, uint64_t, uint64_t, uint64_t, uint64_t
);
PluginHttpResult pandar_plugin_firmware_next_status_override(
    void*, const uint8_t*, std::size_t
);
PluginFirmwareCallbackResult pandar_plugin_firmware_next_callback(void*, uint64_t);
void pandar_plugin_firmware_cancel_generation(void*, uint64_t);
void pandar_plugin_firmware_stop(void*);
PluginHttpResult pandar_plugin_submit_print(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    int64_t,
    bool,
    bool,
    int32_t,
    bool,
    int32_t,
    int32_t,
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
PluginHttpResult pandar_plugin_submit_h2c_auto_nozzle_mapping(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_operation_json_from_gcode(const uint8_t*, std::size_t);
PluginHttpResult pandar_plugin_classify_status_request(const uint8_t*, std::size_t);
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
    std::string country_code;
    std::string token;
    std::string user_id;
    std::string user_name;
    std::string avatar;
    std::string profile_json;
    std::int32_t account_session_kind = 0;
    std::string tenant_id;
    std::string hub_url = "http://127.0.0.1:8080";
    std::string frontend_url = "http://localhost:3000";
    void* plugin_core = nullptr;
    void* connection_session() const { return pandar_plugin_core_connection_session(plugin_core); }
    void* firmware_session() const { return pandar_plugin_core_firmware_session(plugin_core); }
    std::uint64_t account_identity() const { return pandar_plugin_core_account_identity(plugin_core); }
    bool on_worker_thread() const {
        const auto current = std::this_thread::get_id();
        return (status_thread.joinable() && status_thread.get_id() == current) ||
            (firmware_thread.joinable() && firmware_thread.get_id() == current) ||
            (model_task_thread.joinable() && model_task_thread.get_id() == current);
    }
    AgentLifetime lifetime;
    mutable std::mutex trace_mutex;
    mutable std::mutex status_mutex;
    mutable std::mutex printer_refresh_request_mutex;
    mutable std::recursive_mutex printer_refresh_mutex;
    mutable std::recursive_mutex account_mutex;
    mutable std::mutex no_auth_refresh_mutex;
    std::atomic<std::uint64_t> account_config_epoch = 0;
    mutable std::recursive_timed_mutex callback_mutex;
    BBL::OnPrinterConnectedFn on_printer_connected;
    BBL::OnServerConnectedFn on_server_connected;
    BBL::OnLocalConnectedFn on_local_connect;
    BBL::OnMessageFn on_message;
    BBL::OnMessageFn on_local_message;
    BBL::OnMsgArrivedFn on_ssdp_message;
    BBL::OnUserLoginFn on_user_login;
    BBL::OnHttpErrorFn on_http_error;
    BBL::GetCountryCodeFn get_country_code;
    BBL::GetSubscribeFailureFn on_subscribe_failure;
    BBL::OnMessageFn on_user_message;
    BBL::QueueOnMainFn queue_on_main;
    BBL::OnServerErrFn on_server_error;
    std::thread status_thread;
    std::atomic<bool> status_thread_stop = false;
    std::atomic<std::uint64_t> status_wake_generation = 0;
    std::mutex status_thread_mutex;
    std::condition_variable status_thread_wake;
    std::thread firmware_thread;
    std::atomic<bool> firmware_thread_stop = false;
    std::thread model_task_thread;
    std::mutex model_task_mutex;
    std::condition_variable model_task_wake;
    bool model_task_stop = false;
    bool model_task_busy = false;
    std::function<void()> model_task_job;
    bool hub_configured = false;
    bool frontend_configured = false;
};

void start_model_task_worker(Agent*);
void stop_model_task_worker(Agent*);

struct FirmwareObservationTicket {
    std::uint64_t generation = 0;
    std::uint64_t sequence = 0;
};


} // namespace pandar::network_plugin
