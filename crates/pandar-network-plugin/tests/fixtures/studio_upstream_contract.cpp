#include <cstddef>
#include <cstdlib>
#include <cstdint>
#include <iostream>
#include <string>
#include <type_traits>
#include <utility>

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#define BBL PandarBBL
#define Slic3r PandarSlic3r
#include "shim_file_transfer_types.hpp"
#undef Slic3r
#undef BBL

#include "slic3r/Utils/NetworkAgent.hpp"
#include "slic3r/Utils/FileTransferUtils.hpp"

#ifdef PANDAR_CONTRACT_CHECK_TYPES
#include "studio_upstream_type_contract.hpp"
#endif

namespace {

std::string loader_error()
{
#ifdef _WIN32
    const DWORD error = GetLastError();
    char* message{};
    const DWORD length = FormatMessageA(
        FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM |
            FORMAT_MESSAGE_IGNORE_INSERTS,
        nullptr,
        error,
        0,
        reinterpret_cast<char*>(&message),
        0,
        nullptr
    );
    const std::string detail = length && message ? std::string(message, length) : "unknown error";
    if (message) LocalFree(message);
    return "Windows error " + std::to_string(error) + ": " + detail;
#else
    const char* error = dlerror();
    return error ? error : "unknown dynamic loader error";
#endif
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
        if (!handle_) fail("failed to load plugin: " + loader_error());
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
        if (!symbol) {
            fail(std::string("missing plugin symbol: ") + name + ": " + loader_error());
        }
        return reinterpret_cast<Function>(symbol);
    }

private:
    [[noreturn]] static void fail(const std::string& message)
    {
        std::cerr << message << '\n';
        std::exit(10);
    }

#ifdef _WIN32
    HMODULE handle_{};
#else
    void* handle_{};
#endif
};

[[noreturn]] void fail(const std::string& message)
{
    std::cerr << message << '\n';
    std::exit(11);
}

class Agent {
public:
    explicit Agent(const Library& library) : library_(library)
    {
        create_ = library_.require<Slic3r::func_create_agent>("bambu_network_create_agent");
        destroy_ = library_.require<Slic3r::func_destroy_agent>("bambu_network_destroy_agent");
        value_ = create_(std::string{});
        if (!value_) fail("bambu_network_create_agent returned null");
    }

    ~Agent() { destroy_(value_); }

    void* get() const { return value_; }

private:
    const Library& library_;
    Slic3r::func_create_agent create_{};
    Slic3r::func_destroy_agent destroy_{};
    void* value_{};
};

void check_version(const Library& library)
{
    const auto get_version =
        library.require<Slic3r::func_get_version>("bambu_network_get_version");
    const std::string version = get_version();
    if (version != PANDAR_STUDIO_REPORTED_NETWORK_AGENT_VERSION) {
        fail(
            "network-agent version mismatch: expected "
            PANDAR_STUDIO_REPORTED_NETWORK_AGENT_VERSION ", got " + version
        );
    }
}

void check_bind(const Library& library)
{
    Agent agent(library);
    const auto bind = library.require<Slic3r::func_bind>("bambu_network_bind");
    bool callback_called = false;
    BBL::OnUpdateStatusFn callback = [&callback_called](int, int, std::string) {
        callback_called = true;
    };
    const int result = bind(
        agent.get(),
        "127.0.0.1",
        "contract-device",
#if defined(PANDAR_STUDIO_BIND_MODEL_ARGUMENT)
        "contract-model",
#endif
        "contract-sec-link",
        "UTC",
        false,
        std::move(callback)
    );
    if (result != BAMBU_NETWORK_ERR_BIND_FAILED) fail("bind must return explicit unsupported failure");
    if (callback_called) fail("unsupported bind must not invoke callback");
}

void check_print(const Library& library, const char* artifact)
{
    Agent agent(library);
    const auto change_user =
        library.require<Slic3r::func_change_user>("bambu_network_change_user");
    const auto get_print_info =
        library.require<Slic3r::func_get_user_print_info>("bambu_network_get_user_print_info");
    const auto start_print =
        library.require<Slic3r::func_start_print>("bambu_network_start_print");
    if (change_user(
            agent.get(),
            R"({"token":"contract-token","user_id":"contract-user","user_name":"Contract"})"
    ) != BAMBU_NETWORK_SUCCESS) {
        fail("failed to install contract print identity");
    }
    unsigned int http_code = 0;
    std::string print_info;
    if (get_print_info(agent.get(), &http_code, &print_info) != BAMBU_NETWORK_SUCCESS ||
        http_code != 200 || print_info.find("contract-device") == std::string::npos) {
        fail("failed to refresh the contract printer cache");
    }
    BBL::PrintParams params{};
    params.dev_id = "contract-device";
    params.task_name = "contract-task";
    params.project_name = "contract-project";
    params.filename = artifact;
    params.plate_index = 713;
    params.print_type = "from_normal";
    params.task_bed_type = "cool_plate";
    params.task_use_ams = true;
    params.task_bed_leveling = true;
    params.task_flow_cali = true;
    params.task_record_timelapse = true;
    params.auto_bed_leveling = 1;
    params.auto_flow_cali = 2;
    params.auto_offset_cali = 1;
    params.ams_mapping = "[17,23]";
    params.ams_mapping2 = R"([{"ams_id":17,"slot_id":23}])";
    params.ams_mapping_info = R"([{"ams":17,"targetColor":"contract-tail","filamentId":"contract-filament","filamentType":"PLA","nozzleId":29}])";
#if defined(PANDAR_STUDIO_PRINT_SVC_CONTEXT)
    params.svc_context = "contract-service-context";
#endif
    bool finished = false;
    BBL::OnUpdateStatusFn update = [&finished](int stage, int code, std::string) {
        finished = stage == BBL::PrintingStageFinished && code == BAMBU_NETWORK_SUCCESS;
    };
    const int result = start_print(agent.get(), std::move(params), std::move(update), {}, {});
    if (result != BAMBU_NETWORK_SUCCESS || !finished) {
        fail("target PrintParams call did not complete through the observable Hub sink");
    }
}

#include "studio_upstream_ft_contract.hpp"

} // namespace

int main(int argc, char** argv)
{
    if (argc < 3 || argc > 4) {
        std::cerr << "usage: studio_upstream_contract <plugin> <version|bind|print|ft> [artifact]\n";
        return 2;
    }
    const std::string mode = argv[2];
    const Library library(argv[1]);
    if (mode == "version") check_version(library);
    else if (mode == "bind") check_bind(library);
    else if (mode == "print") {
        if (argc != 4) fail("print contract requires an artifact path");
        check_print(library, argv[3]);
    }
    else if (mode == "ft") check_ft(library);
    else fail("unknown contract mode: " + mode);
    std::cout << "contract_mode=" << mode << " ok\n";
    return 0;
}
