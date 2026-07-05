#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <cstdint>
#include <cstring>
#include <string>

namespace {

constexpr char kTargetPath[] = "v1/analysis-st/tag/";
constexpr char kPatchedPath[] = "v1/analysis-st/nag/";
static_assert(sizeof(kTargetPath) == sizeof(kPatchedPath));

std::wstring appdata_log_path()
{
    wchar_t appdata[MAX_PATH] = {};
    DWORD len = GetEnvironmentVariableW(L"APPDATA", appdata, MAX_PATH);
    if (len == 0 || len >= MAX_PATH) {
        return L"pandar-studio-dev-hook.log";
    }
    return std::wstring(appdata) + L"\\BambuStudio\\pandar-studio-dev-hook.log";
}

void append_log(const char* message)
{
    HANDLE file = CreateFileW(
        appdata_log_path().c_str(),
        FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        nullptr,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return;
    }

    DWORD written = 0;
    WriteFile(file, message, static_cast<DWORD>(std::strlen(message)), &written, nullptr);
    WriteFile(file, "\r\n", 2, &written, nullptr);
    CloseHandle(file);
}

bool dev_hook_enabled()
{
    wchar_t value[8] = {};
    DWORD len = GetEnvironmentVariableW(L"PANDAR_STUDIO_DEV_LOG_LOCAL_KEY", value, 8);
    return len == 1 && value[0] == L'1';
}

std::size_t image_size(HMODULE module)
{
    auto* base = reinterpret_cast<std::uint8_t*>(module);
    auto* dos = reinterpret_cast<IMAGE_DOS_HEADER*>(base);
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) {
        return 0;
    }

    auto* nt = reinterpret_cast<IMAGE_NT_HEADERS*>(base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE) {
        return 0;
    }

    return nt->OptionalHeader.SizeOfImage;
}

int patch_bambu_studio(HMODULE module)
{
    auto* base = reinterpret_cast<std::uint8_t*>(module);
    std::size_t size = image_size(module);
    if (size == 0 || size < sizeof(kTargetPath)) {
        return 0;
    }

    int patched = 0;
    for (std::size_t offset = 0; offset + sizeof(kTargetPath) <= size; ++offset) {
        if (std::memcmp(base + offset, kTargetPath, sizeof(kTargetPath)) != 0) {
            continue;
        }

        DWORD old_protect = 0;
        if (VirtualProtect(base + offset, sizeof(kTargetPath), PAGE_READWRITE, &old_protect)) {
            std::memcpy(base + offset, kPatchedPath, sizeof(kPatchedPath));
            FlushInstructionCache(GetCurrentProcess(), base + offset, sizeof(kTargetPath));
            DWORD ignored = 0;
            VirtualProtect(base + offset, sizeof(kTargetPath), old_protect, &ignored);
            patched += 1;
        }
    }

    return patched;
}

DWORD WINAPI patch_thread(void*)
{
    if (!dev_hook_enabled()) {
        return 0;
    }

    append_log("Pandar Studio dev hook enabled; waiting for BambuStudio.dll");
    for (int attempt = 0; attempt < 200; ++attempt) {
        HMODULE module = GetModuleHandleW(L"BambuStudio.dll");
        if (module != nullptr) {
            int patched = patch_bambu_studio(module);
            if (patched > 0) {
                append_log("Patched Bambu Studio log key endpoint; new logs should use the local fallback key");
            } else {
                append_log("BambuStudio.dll loaded but log key endpoint was not found");
            }
            return 0;
        }
        Sleep(50);
    }

    append_log("Timed out waiting for BambuStudio.dll");
    return 0;
}

HMODULE original_swscale()
{
    static HMODULE module = LoadLibraryW(L"swscale8original.dll");
    return module;
}

FARPROC original_proc(const char* name)
{
    HMODULE module = original_swscale();
    if (module == nullptr) {
        append_log("Failed to load swscale8original.dll");
        return nullptr;
    }
    FARPROC proc = GetProcAddress(module, name);
    if (proc == nullptr) {
        append_log("Failed to resolve swscale original export");
    }
    return proc;
}

}

extern "C" __declspec(dllexport) void* __cdecl sws_getCachedContext(
    void* context,
    int src_w,
    int src_h,
    int src_format,
    int dst_w,
    int dst_h,
    int dst_format,
    int flags,
    void* src_filter,
    void* dst_filter,
    const double* param)
{
    using Fn = void*(__cdecl*)(
        void*, int, int, int, int, int, int, int, void*, void*, const double*);
    auto fn = reinterpret_cast<Fn>(original_proc("sws_getCachedContext"));
    if (fn == nullptr) {
        return nullptr;
    }
    return fn(
        context,
        src_w,
        src_h,
        src_format,
        dst_w,
        dst_h,
        dst_format,
        flags,
        src_filter,
        dst_filter,
        param);
}

extern "C" __declspec(dllexport) int __cdecl sws_scale(
    void* context,
    const std::uint8_t* const src_slice[],
    const int src_stride[],
    int src_slice_y,
    int src_slice_h,
    std::uint8_t* const dst[],
    const int dst_stride[])
{
    using Fn = int(__cdecl*)(
        void*, const std::uint8_t* const*, const int*, int, int, std::uint8_t* const*, const int*);
    auto fn = reinterpret_cast<Fn>(original_proc("sws_scale"));
    if (fn == nullptr) {
        return 0;
    }
    return fn(context, src_slice, src_stride, src_slice_y, src_slice_h, dst, dst_stride);
}

extern "C" __declspec(dllexport) void __cdecl sws_freeContext(void* context)
{
    using Fn = void(__cdecl*)(void*);
    auto fn = reinterpret_cast<Fn>(original_proc("sws_freeContext"));
    if (fn != nullptr) {
        fn(context);
    }
}

extern "C" BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID)
{
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(instance);
        if (dev_hook_enabled()) {
            HMODULE module = GetModuleHandleW(L"BambuStudio.dll");
            if (module != nullptr) {
                int patched = patch_bambu_studio(module);
                if (patched > 0) {
                    append_log("Patched Bambu Studio log key endpoint during DLL attach");
                    return TRUE;
                }
            }
        }

        HANDLE thread = CreateThread(nullptr, 0, patch_thread, nullptr, 0, nullptr);
        if (thread != nullptr) {
            CloseHandle(thread);
        }
    }
    return TRUE;
}
