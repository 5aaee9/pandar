#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <winver.h>

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "plugin_download_hook.hpp"

namespace {

constexpr wchar_t kPluginPackage[] = L"networking_plugins.zip";
constexpr wchar_t kHookPackageSubpath[] =
    L"\\Pandar\\studio-hook\\networking_plugins.zip";

using MoveFileExWFn = BOOL(WINAPI*)(LPCWSTR, LPCWSTR, DWORD);
using MoveFileWFn = BOOL(WINAPI*)(LPCWSTR, LPCWSTR);

MoveFileExWFn original_move_file_ex = nullptr;
MoveFileWFn original_move_file = nullptr;
std::wstring cached_plugin_package;

std::wstring log_path()
{
    wchar_t appdata[MAX_PATH] = {};
    DWORD len = GetEnvironmentVariableW(L"APPDATA", appdata, MAX_PATH);
    if (len == 0 || len >= MAX_PATH)
        return L"pandar-studio-hook.log";
    return std::wstring(appdata) + L"\\BambuStudio\\pandar-studio-hook.log";
}

void append_log(const char* message)
{
    HANDLE file = CreateFileW(
        log_path().c_str(), FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
        OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE)
        return;
    DWORD written = 0;
    WriteFile(file, message, static_cast<DWORD>(std::strlen(message)), &written, nullptr);
    WriteFile(file, "\r\n", 2, &written, nullptr);
    CloseHandle(file);
}

bool is_target_studio_version(HMODULE module)
{
    wchar_t module_path[MAX_PATH] = {};
    if (GetModuleFileNameW(module, module_path, MAX_PATH) == 0)
        return false;

    DWORD ignored = 0;
    DWORD size = GetFileVersionInfoSizeW(module_path, &ignored);
    if (size == 0)
        return false;
    std::vector<std::uint8_t> version_info(size);
    if (!GetFileVersionInfoW(module_path, 0, size, version_info.data()))
        return false;

    VS_FIXEDFILEINFO* fixed = nullptr;
    UINT fixed_size = 0;
    if (!VerQueryValueW(
            version_info.data(), L"\\", reinterpret_cast<void**>(&fixed), &fixed_size) ||
        fixed == nullptr || fixed_size < sizeof(VS_FIXEDFILEINFO))
        return false;

    return HIWORD(fixed->dwFileVersionMS) == 2 &&
        LOWORD(fixed->dwFileVersionMS) == 7 &&
        HIWORD(fixed->dwFileVersionLS) == 1;
}

std::wstring plugin_package_path()
{
    wchar_t local_appdata[MAX_PATH] = {};
    DWORD len = GetEnvironmentVariableW(L"LOCALAPPDATA", local_appdata, MAX_PATH);
    if (len == 0 || len >= MAX_PATH)
        return {};
    return std::wstring(local_appdata) + kHookPackageSubpath;
}

bool is_plugin_package_destination(LPCWSTR destination)
{
    if (destination == nullptr)
        return false;
    wchar_t temp_path[MAX_PATH] = {};
    DWORD temp_len = GetTempPathW(MAX_PATH, temp_path);
    if (temp_len == 0 || temp_len >= MAX_PATH)
        return false;
    std::wstring expected(temp_path);
    expected += kPluginPackage;

    wchar_t full_destination[MAX_PATH] = {};
    DWORD full_len = GetFullPathNameW(destination, MAX_PATH, full_destination, nullptr);
    if (full_len == 0 || full_len >= MAX_PATH)
        return false;
    return _wcsicmp(full_destination, expected.c_str()) == 0;
}

bool replace_downloaded_package(LPCWSTR downloaded)
{
    if (downloaded == nullptr || cached_plugin_package.empty() ||
        GetFileAttributesW(cached_plugin_package.c_str()) == INVALID_FILE_ATTRIBUTES) {
        append_log("Blocked Studio network plugin install because the verified Pandar package is unavailable");
        SetLastError(ERROR_FILE_NOT_FOUND);
        return false;
    }
    if (!CopyFileW(cached_plugin_package.c_str(), downloaded, FALSE)) {
        append_log("Failed to substitute the Studio network plugin download with the Pandar package");
        return false;
    }
    return true;
}

BOOL WINAPI hooked_move_file_ex(LPCWSTR existing, LPCWSTR destination, DWORD flags)
{
    if (!is_plugin_package_destination(destination))
        return original_move_file_ex(existing, destination, flags);
    if (!replace_downloaded_package(existing))
        return FALSE;
    BOOL result = original_move_file_ex(existing, destination, flags);
    append_log(result
        ? "Substituted Studio network plugin download with the verified Pandar package"
        : "Failed to publish the substituted Pandar plugin package");
    return result;
}

BOOL WINAPI hooked_move_file(LPCWSTR existing, LPCWSTR destination)
{
    if (!is_plugin_package_destination(destination))
        return original_move_file(existing, destination);
    if (!replace_downloaded_package(existing))
        return FALSE;
    BOOL result = original_move_file(existing, destination);
    append_log(result
        ? "Substituted Studio network plugin download with the verified Pandar package"
        : "Failed to publish the substituted Pandar plugin package");
    return result;
}

bool patch_import(HMODULE module, const char* function_name, void* replacement, void** original)
{
    auto* base = reinterpret_cast<std::uint8_t*>(module);
    auto* dos = reinterpret_cast<IMAGE_DOS_HEADER*>(base);
    if (dos->e_magic != IMAGE_DOS_SIGNATURE)
        return false;
    auto* nt = reinterpret_cast<IMAGE_NT_HEADERS*>(base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE)
        return false;

    const auto& directory =
        nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (directory.VirtualAddress == 0)
        return false;
    auto* descriptor = reinterpret_cast<IMAGE_IMPORT_DESCRIPTOR*>(
        base + directory.VirtualAddress);
    bool patched = false;
    for (; descriptor->Name != 0; ++descriptor) {
        if (descriptor->OriginalFirstThunk == 0)
            continue;
        auto* names = reinterpret_cast<IMAGE_THUNK_DATA*>(
            base + descriptor->OriginalFirstThunk);
        auto* addresses = reinterpret_cast<IMAGE_THUNK_DATA*>(
            base + descriptor->FirstThunk);
        for (; names->u1.AddressOfData != 0; ++names, ++addresses) {
            if (IMAGE_SNAP_BY_ORDINAL(names->u1.Ordinal))
                continue;
            auto* import = reinterpret_cast<IMAGE_IMPORT_BY_NAME*>(
                base + names->u1.AddressOfData);
            if (std::strcmp(
                    reinterpret_cast<const char*>(import->Name), function_name) != 0)
                continue;

            auto** slot = reinterpret_cast<void**>(&addresses->u1.Function);
            if (*slot == replacement) {
                patched = true;
                continue;
            }
            DWORD old_protect = 0;
            if (!VirtualProtect(slot, sizeof(void*), PAGE_READWRITE, &old_protect))
                continue;
            if (*original == nullptr)
                *original = *slot;
            *slot = replacement;
            DWORD ignored = 0;
            VirtualProtect(slot, sizeof(void*), old_protect, &ignored);
            FlushInstructionCache(GetCurrentProcess(), slot, sizeof(void*));
            patched = true;
        }
    }
    return patched;
}

int patch_loaded_modules()
{
    HANDLE snapshot = CreateToolhelp32Snapshot(
        TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, GetCurrentProcessId());
    if (snapshot == INVALID_HANDLE_VALUE)
        return 0;
    MODULEENTRY32W entry = {};
    entry.dwSize = sizeof(entry);
    int patched = 0;
    if (Module32FirstW(snapshot, &entry)) {
        do {
            patched += patch_import(
                entry.hModule, "MoveFileExW", reinterpret_cast<void*>(hooked_move_file_ex),
                reinterpret_cast<void**>(&original_move_file_ex));
            patched += patch_import(
                entry.hModule, "MoveFileW", reinterpret_cast<void*>(hooked_move_file),
                reinterpret_cast<void**>(&original_move_file));
        } while (Module32NextW(snapshot, &entry));
    }
    CloseHandle(snapshot);
    return patched;
}

} // namespace

void install_plugin_download_hook(HMODULE studio_module)
{
    cached_plugin_package = plugin_package_path();
    if (!is_target_studio_version(studio_module)) {
        append_log("Pandar plugin download hook disabled for an unsupported Studio version");
        return;
    }

    int patched = patch_loaded_modules();
    if (patched == 0) {
        append_log("Pandar plugin download hook could not find the Windows rename imports");
        return;
    }
    append_log("Pandar plugin download hook enabled for Studio 02.07.01.x");
}
