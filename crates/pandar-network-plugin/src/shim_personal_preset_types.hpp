#pragma once

#include <cstddef>
#include <cstdint>

namespace pandar::network_plugin {

struct PresetBytes {
    const uint8_t* ptr;
    std::size_t len;
};

struct PresetEntry {
    PresetBytes key;
    PresetBytes value;
};

using PresetEntryVisitor = int32_t (*)(void*, PresetBytes, PresetBytes);
using PresetCheckCallback = int32_t (*)(void*, const PresetEntry*, std::size_t);
using PresetIntCallback = int32_t (*)(void*, int32_t);

struct PresetCallbacks {
    void* context;
    PresetCheckCallback check;
    PresetIntCallback progress;
    PresetIntCallback cancel;
    PresetIntCallback current;
};

struct PersonalPresetAccount {
    PresetBytes hub_url;
    PresetBytes token;
    PresetBytes user_id;
    uint64_t account_epoch;
    uint64_t config_epoch;
    int32_t session_kind;
    int32_t transition_pending;
    uint64_t identity;
};

struct PresetResult {
    int32_t status;
    uint32_t http_code;
    int64_t updated_time;
    int32_t code;
    uint8_t* id_ptr;
    std::size_t id_len;
    std::size_t id_cap;
};

extern "C" {
PresetResult pandar_plugin_personal_preset_mutate(
    int32_t, const PersonalPresetAccount*, PresetBytes, PresetBytes,
    const PresetEntry*, std::size_t
);
int32_t pandar_plugin_personal_preset_list(
    const PersonalPresetAccount*, PresetBytes, PresetCallbacks
);
int32_t pandar_plugin_personal_preset_drain(
    const PersonalPresetAccount*, void*, PresetEntryVisitor
);
void pandar_plugin_personal_preset_reset(uint64_t);
}

} // namespace pandar::network_plugin
