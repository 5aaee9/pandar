#pragma once

namespace pandar::network_plugin {

using PluginRuntimeConfigVisitor = void (*)(
    void*,
    const uint8_t*, std::size_t, bool,
    const uint8_t*, std::size_t, bool
);

struct PluginAccountBytes {
    const std::uint8_t* ptr;
    std::size_t len;
};

struct PluginAccountView {
    PluginAccountBytes config_dir;
    PluginAccountBytes hub_url;
    PluginAccountBytes token;
    PluginAccountBytes user_id;
    PluginAccountBytes user_name;
    PluginAccountBytes avatar;
    PluginAccountBytes profile_json;
    std::uint64_t account_epoch;
    std::uint64_t config_epoch;
    std::int32_t session_kind;
    std::int32_t transition_pending;
};

struct PluginAccountMutation {
    std::int32_t action;
    std::int32_t notification;
    PluginAccountBytes hub_url;
    PluginAccountBytes token;
    PluginAccountBytes user_id;
    PluginAccountBytes user_name;
    PluginAccountBytes avatar;
    PluginAccountBytes profile_json;
    std::int32_t session_kind;
    PluginAccountBytes error_body;
    std::uint32_t http_code;
};

using PluginAccountTransaction = std::int32_t (*)(
    void*, const PluginAccountView*, PluginAccountMutation*
);
using PluginWithCurrentAccount = std::int32_t (*)(
    void*, void*, PluginAccountTransaction
);
extern "C" std::int32_t with_current_account(
    void*, void*, PluginAccountTransaction
);
using PluginAccountTenantVisitor = void (*)(void*, const uint8_t*, std::size_t);

struct PluginAccountSessionBridge {
    void (*replace)(
        void*, PluginAccountBytes, PluginAccountBytes, PluginAccountBytes,
        PluginAccountBytes, PluginAccountBytes, PluginAccountBytes, std::int32_t
    );
    void (*clear)(void*);
    void (*set_hub_url)(void*, PluginAccountBytes);
    void (*invoke_user_login)(void*, std::int32_t, bool);
    void (*invoke_http_error)(void*, std::uint32_t, PluginAccountBytes);
    void (*reset_personal_presets)(void*);
};

struct PluginDispatchBridge;

struct PluginLifecycleResult {
    PluginHttpResult http;
    std::int32_t account_event;
    std::int32_t report_http_error;
};

enum class StudioDisposition : std::uint32_t {
    InitLog = 1,
    SetCert = 2,
    UpdateCert = 3,
    InstallCert = 4,
    StartSubscribe = 5,
    StopSubscribe = 6,
    Consent = 7,
    LocalPrintWithRecord = 8,
    UserPresets = 9,
    RequestSettingId = 10,
    PutSetting = 11,
    GetSettingList = 12,
    GetSettingList2 = 13,
    DeleteSetting = 14,
    ExtraHttpHeader = 15,
    UserMessages = 16,
    UserTaskReport = 17,
    HmsSnapshot = 18,
    DesignStaffPick = 19,
    StartPublish = 20,
    ModelPublishUrl = 21,
    ModelMallHome = 22,
    ModelMallDetail = 23,
    PutModelRating = 24,
    OssConfig = 25,
    PutRatingPicture = 26,
    GetModelRating = 27,
    MakerWorldPreference = 28,
    MakerWorldForYou = 29,
    GetFilaments = 30,
    CreateFilament = 31,
    UpdateFilament = 32,
    DeleteFilament = 33,
    GetFilamentConfig = 34,
    TrackEnable = 35,
    TrackRemoveFiles = 36,
    TrackEvent = 37,
    TrackHeader = 38,
    TrackUpdateProperty = 39,
    TrackGetProperty = 40,
    SendGcodeToSdcard = 41,
    LocalPrint = 42,
    SdcardPrint = 43,
    EnableMultiMachine = 44,
    StartDiscovery = 45,
    PingBind = 46,
    BindDetect = 47,
    Bind = 48,
    Unbind = 49,
    BindTicket = 50,
    BindStatus = 51,
    ModifyPrinterName = 52,
    StudioInfoUnavailable = 53,
};

enum class AccountPolicyAction : std::int32_t {
    Failure = -1,
    None = 0,
    Apply = 1,
    Reset = 2,
    Logout = 3,
    Login = 4,
};

extern "C" {
void* pandar_plugin_account_session_create();
void pandar_plugin_account_session_destroy(void*);
void pandar_plugin_account_session_apply_lifecycle_result(
    void*, const PluginLifecycleResult*
);
int32_t pandar_plugin_account_session_apply(
    void*, void*, void*, const PluginAccountSessionBridge*, void*,
    const PluginAccountView*, const PluginAccountMutation*
);
void pandar_plugin_account_session_drain(
    void*, void*, const PluginDispatchBridge*, const PluginAccountSessionBridge*,
    void*, void*, PluginWithCurrentAccount
);
int32_t pandar_plugin_account_runtime_config(void*, PluginRuntimeConfigVisitor);
int32_t pandar_plugin_account_profile_tenant_id(
    const uint8_t*, std::size_t, void*, PluginAccountTenantVisitor
);
bool pandar_plugin_account_debug_consistent(bool);
PluginHttpResult pandar_plugin_account_login_envelope(
    bool,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_account_local_base_url(const uint8_t*, std::size_t);
uint64_t pandar_plugin_account_identity_create();
bool pandar_plugin_account_observe_login(
    uint64_t, uint64_t, const uint8_t*, std::size_t
);
void pandar_plugin_account_login_observation_clear(uint64_t);
int32_t pandar_plugin_account_response_action(int32_t);
int32_t pandar_plugin_account_response_status(int32_t);
int32_t pandar_plugin_account_commit_action(
    uint64_t, uint64_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    bool
);
PluginLifecycleResult pandar_plugin_account_no_auth_bootstrap(
    void*, bool, std::uint64_t, void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_logout(
    void*, std::uint64_t, bool, void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_change_user(
    void*, std::uint64_t, const uint8_t*, std::size_t,
    void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_exchange_ticket(
    const uint8_t*, std::size_t, void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_profile(
    const uint8_t*, std::size_t, void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_load_persisted(
    void*, PluginWithCurrentAccount
);
PluginLifecycleResult pandar_plugin_account_refresh_runtime(
    void*, PluginWithCurrentAccount
);
PluginHttpResult pandar_plugin_account_studio_info_url(
    bool, bool, const uint8_t*, std::size_t
);
PluginHttpResult pandar_plugin_studio_disposition(uint32_t, bool);
PluginHttpResult pandar_plugin_studio_request_admitted(bool, bool);
PluginHttpResult pandar_plugin_studio_firmware_catalog_result(
    int32_t, uint32_t, const uint8_t*, std::size_t, bool
);
PluginHttpResult pandar_plugin_studio_printer_operation_result(
    int32_t, uint32_t, const uint8_t*, std::size_t, bool
);
PluginHttpResult pandar_plugin_studio_status_delivery_result(bool);
PluginHttpResult pandar_plugin_studio_file_transfer_unavailable();
}

} // namespace pandar::network_plugin
