#pragma once

namespace pandar::network_plugin {

extern "C" {

struct PluginBytes {
    const std::uint8_t* ptr;
    std::size_t len;
};

struct PluginStudioSnapshot {
    PluginBytes hub_url;
    PluginBytes token;
    PluginBytes printer_id;
    std::uint8_t printer_authorized;
    std::uint8_t account_transition_pending;
    std::uint64_t account_epoch;
    std::uint64_t cache_generation;
    std::uint64_t firmware_generation;
};

struct PluginStudioPrintParams {
    PluginStudioSnapshot snapshot;
    PluginBytes dev_id;
    PluginBytes task_name;
    PluginBytes project_name;
    PluginBytes preset_name;
    PluginBytes filename;
    PluginBytes config_filename;
    std::int32_t plate_index;
    PluginBytes ftp_folder;
    PluginBytes ftp_file;
    PluginBytes ftp_file_md5;
    PluginBytes nozzle_mapping;
    PluginBytes ams_mapping;
    PluginBytes ams_mapping2;
    PluginBytes ams_mapping_info;
    PluginBytes nozzles_info;
    PluginBytes connection_type;
    PluginBytes comments;
    std::int32_t origin_profile_id;
    std::int32_t stl_design_id;
    PluginBytes origin_model_id;
    PluginBytes print_type;
    PluginBytes dst_file;
    PluginBytes dev_name;
    PluginBytes dev_ip;
    std::uint8_t use_ssl_for_ftp;
    std::uint8_t use_ssl_for_mqtt;
    PluginBytes username;
    PluginBytes password;
    std::uint8_t task_bed_leveling;
    std::uint8_t task_flow_cali;
    std::uint8_t task_vibration_cali;
    std::uint8_t task_layer_inspect;
    std::uint8_t task_record_timelapse;
    std::uint8_t task_timelapse_use_internal;
    std::uint8_t task_use_ams;
    PluginBytes task_bed_type;
    PluginBytes extra_options;
    std::int32_t auto_bed_leveling;
    std::int32_t auto_flow_cali;
    std::int32_t auto_offset_cali;
    std::int32_t extruder_cali_manual_mode;
    std::uint8_t task_ext_change_assist;
    std::uint8_t try_emmc_print;
    PluginBytes svc_context;
    PluginBytes slicer_uid;
};

struct PluginStudioCallbacks {
    void* context;
    void (*update)(void*, std::int32_t, std::int32_t, const std::uint8_t*, std::size_t);
    std::int32_t (*cancelled)(void*);
    std::int32_t (*wait)(void*, std::int32_t, const std::uint8_t*, std::size_t);
    std::int32_t (*snapshot)(void*, PluginStudioSnapshot*);
};

struct PluginStudioAccount {
    PluginStudioSnapshot snapshot;
    void* context;
    std::int32_t (*current_snapshot)(void*, PluginStudioSnapshot*);
};

struct PluginStudioTaskQuery {
    PluginBytes dev_id;
    std::int32_t status;
    std::int32_t offset;
    std::int32_t limit;
};

struct PluginStudioPlateResult {
    PluginHttpResult http;
    std::int32_t plate_index;
};

struct PluginStudioModelTask {
    std::int32_t job_id;
    std::int32_t design_id;
    std::int32_t profile_id;
    std::int32_t instance_id;
    PluginBytes task_id;
    PluginBytes model_id;
    PluginBytes model_name;
    PluginBytes profile_name;
};

using StudioModelTaskVisitor = std::int32_t (*)(void*, const PluginStudioModelTask*);
using StudioModelTaskCancelled = std::int32_t (*)(void*);

std::int32_t pandar_plugin_studio_start_print(
    const PluginStudioPrintParams*,
    PluginStudioCallbacks
);
PluginHttpResult pandar_plugin_studio_get_tasks(
    const PluginStudioAccount*,
    const PluginStudioTaskQuery*
);
PluginHttpResult pandar_plugin_studio_get_tasks_with_session(
    void*, const PluginStudioAccount*, std::uint64_t, std::int32_t,
    void*, PluginWithCurrentAccount, const PluginStudioTaskQuery*
);
PluginStudioPlateResult pandar_plugin_studio_get_plate(
    const PluginStudioAccount*,
    PluginBytes
);
PluginStudioPlateResult pandar_plugin_studio_get_plate_with_session(
    void*, const PluginStudioAccount*, std::uint64_t, std::int32_t,
    void*, PluginWithCurrentAccount, PluginBytes
);
PluginHttpResult pandar_plugin_studio_get_subtask(
    const PluginStudioAccount*,
    PluginBytes
);
PluginHttpResult pandar_plugin_studio_get_subtask_with_session(
    void*, const PluginStudioAccount*, std::uint64_t, std::int32_t,
    void*, PluginWithCurrentAccount, PluginBytes
);
PluginHttpResult pandar_plugin_studio_get_model_task_with_session(
    void*, const PluginStudioAccount*, std::uint64_t, std::int32_t,
    void*, PluginWithCurrentAccount, PluginBytes, void*, StudioModelTaskVisitor,
    void*, StudioModelTaskCancelled
);
PluginHttpResult pandar_plugin_studio_slice_unavailable();
std::int32_t pandar_plugin_core_studio_request_snapshot_current(
    void*,
    const PluginStudioSnapshot*,
    const PluginStudioSnapshot*
);

}

inline PluginBytes plugin_bytes(const std::string& value) {
    return {
        reinterpret_cast<const std::uint8_t*>(value.data()),
        value.size(),
    };
}

} // namespace pandar::network_plugin
