#include <cstdlib>
#include <functional>
#include <iostream>
#include <map>
#include <string>
#include <type_traits>
#include <utility>

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#include "slic3r/Utils/NetworkAgent.hpp"

// Every type below comes from the exact catalog-pinned Studio header.
namespace {

using PresetMap = std::map<std::string, std::map<std::string, std::string>>;
using ValuesMap = std::map<std::string, std::string>;
using GetUserPresets = int (*)(void*, PresetMap*);
using RequestSettingId = std::string (*)(void*, std::string, ValuesMap*, unsigned int*);
using PutSetting = int (*)(void*, std::string, std::string, ValuesMap*, unsigned int*);
using GetSettingList = int (*)(void*, std::string, BBL::ProgressFn, BBL::WasCancelledFn);
using GetSettingList2 =
    int (*)(void*, std::string, BBL::CheckFn, BBL::ProgressFn, BBL::WasCancelledFn);
using DeleteSetting = int (*)(void*, std::string);

static_assert(std::is_same_v<Slic3r::func_get_user_presets, GetUserPresets>);
static_assert(std::is_same_v<Slic3r::func_request_setting_id, RequestSettingId>);
static_assert(std::is_same_v<Slic3r::func_put_setting, PutSetting>);
static_assert(std::is_same_v<Slic3r::func_get_setting_list, GetSettingList>);
static_assert(std::is_same_v<Slic3r::func_get_setting_list2, GetSettingList2>);
static_assert(std::is_same_v<Slic3r::func_delete_setting, DeleteSetting>);
static_assert(std::is_same_v<BBL::ProgressFn, std::function<void(int)>>);
static_assert(std::is_same_v<BBL::WasCancelledFn, std::function<bool()>>);
static_assert(std::is_same_v<BBL::CheckFn, std::function<bool(ValuesMap)>>);

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

class Agent {
public:
    Agent(const Library& library, const char* config_dir)
        : destroy_(library.require<Slic3r::func_destroy_agent>("bambu_network_destroy_agent"))
    {
        const auto create =
            library.require<Slic3r::func_create_agent>("bambu_network_create_agent");
        const auto set_config =
            library.require<Slic3r::func_set_config_dir>("bambu_network_set_config_dir");
        value_ = create("personal-preset-contract-red");
        if (!value_ || set_config(value_, config_dir) != BAMBU_NETWORK_SUCCESS)
            fail("agent setup failed");
    }

    ~Agent() { destroy_(value_); }
    void* get() const { return value_; }

private:
    Slic3r::func_destroy_agent destroy_{};
    void* value_{};
};

} // namespace

int main(int argc, char** argv)
{
    if (argc != 3) fail("usage: personal-presets <plugin> <config-dir>");
    const Library library(argv[1]);
    Agent agent(library, argv[2]);

    const auto get_user_presets = library.require<Slic3r::func_get_user_presets>(
        "bambu_network_get_user_presets"
    );
    const auto request_setting_id = library.require<Slic3r::func_request_setting_id>(
        "bambu_network_request_setting_id"
    );
    const auto put_setting = library.require<Slic3r::func_put_setting>(
        "bambu_network_put_setting"
    );
    const auto get_setting_list = library.require<Slic3r::func_get_setting_list>(
        "bambu_network_get_setting_list"
    );
    const auto get_setting_list2 = library.require<Slic3r::func_get_setting_list2>(
        "bambu_network_get_setting_list2"
    );
    const auto delete_setting = library.require<Slic3r::func_delete_setting>(
        "bambu_network_delete_setting"
    );

    PresetMap presets{
        {"sentinel", {{"type", "print"}}}
    };
    const int drain_rc = get_user_presets(agent.get(), &presets);

    ValuesMap create_values{
        {"type", "print"}, {"version", "2.8.1.55"}, {"updated_time", "11"},
        {"code", "sentinel"}
    };
    unsigned int create_http = 777;
    const std::string created = request_setting_id(
        agent.get(), "Contract Process", &create_values, &create_http
    );

    ValuesMap update_values{
        {"type", "print"}, {"version", "2.8.1.55"}, {"updated_time", "12"},
        {"code", "sentinel"}
    };
    unsigned int update_http = 778;
    const int update_rc = put_setting(
        agent.get(), "setting-contract", "Contract Process", &update_values, &update_http
    );

    int legacy_progress = 0;
    int legacy_cancel = 0;
    const int legacy_list_rc = get_setting_list(
        agent.get(), "2.8.1.55",
        [&](int) { ++legacy_progress; },
        [&] { ++legacy_cancel; return false; }
    );

    int checks = 0;
    int progress = 0;
    int cancel = 0;
    ValuesMap check_info;
    const int list2_rc = get_setting_list2(
        agent.get(), "2.8.1.55",
        [&](ValuesMap info) {
            ++checks;
            check_info = std::move(info);
            return false;
        },
        [&](int) { ++progress; },
        [&] { ++cancel; return true; }
    );
    const int delete_rc = delete_setting(agent.get(), "setting-contract");

    const bool explicit_unavailable =
        drain_rc == BAMBU_NETWORK_ERR_INVALID_RESULT && presets.empty() && created.empty() && create_http == 403 &&
        update_rc == BAMBU_NETWORK_ERR_PUT_SETTING_FAILED && update_http == 403 &&
        legacy_list_rc == -9 && legacy_progress == 0 &&
        legacy_cancel == 0 && list2_rc == -9 && checks == 0 &&
        progress == 0 && cancel == 0 && check_info.empty() &&
        delete_rc == BAMBU_NETWORK_ERR_DEL_SETTING_FAILED;
    if (!explicit_unavailable) fail("personal preset ABI did not fail explicitly without an account");

    presets = {{"sentinel", {{"type", "print"}}}};
    create_http = 779;
    update_http = 780;
    const int invalid_drain_rc = get_user_presets(nullptr, &presets);
    const std::string invalid_created =
        request_setting_id(nullptr, "name", &create_values, &create_http);
    const int invalid_update_rc =
        put_setting(nullptr, "id", "name", &update_values, &update_http);
    const int invalid_legacy_list_rc = get_setting_list(nullptr, "", {}, {});
    const int invalid_list2_rc = get_setting_list2(nullptr, "", {}, {}, {});
    const int invalid_delete_rc = delete_setting(nullptr, "id");
    const bool invalid_handles =
        invalid_drain_rc == BAMBU_NETWORK_ERR_INVALID_HANDLE && presets.empty() &&
        invalid_created.empty() && create_http == 0 &&
        invalid_update_rc == BAMBU_NETWORK_ERR_INVALID_HANDLE && update_http == 0 &&
        invalid_legacy_list_rc == BAMBU_NETWORK_ERR_INVALID_HANDLE &&
        invalid_list2_rc == BAMBU_NETWORK_ERR_INVALID_HANDLE &&
        invalid_delete_rc == BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (!invalid_handles) fail("personal preset invalid-handle contract changed");

    std::cout << "{\"ok\":true,\"contract_state\":\"handled_personal_presets\","
              << "\"calls\":6,\"callbacks_invoked\":0,\"http_code\":403}" << '\n';
}
