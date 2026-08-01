#pragma once

#include "shim_print_types.hpp"
#include "shim_request_snapshot.hpp"

namespace pandar::network_plugin {

struct StudioPrintCallbackContext {
    Agent* agent;
    const std::string* dev_id;
    PrinterRequestSnapshot current_snapshot;
    const BBL::OnUpdateStatusFn* update;
    const BBL::WasCancelledFn* cancelled;
    const BBL::OnWaitFn* wait;
};

inline void studio_print_update(
    void* opaque,
    std::int32_t stage,
    std::int32_t code,
    const std::uint8_t* body,
    std::size_t body_len
) {
    auto* context = static_cast<StudioPrintCallbackContext*>(opaque);
    if (!context || !context->update || !*context->update) return;
    std::string message;
    if (body && body_len > 0) {
        message.assign(reinterpret_cast<const char*>(body), body_len);
    }
    (*context->update)(stage, code, std::move(message));
}

inline std::int32_t studio_print_cancelled(void* opaque) {
    auto* context = static_cast<StudioPrintCallbackContext*>(opaque);
    return context && context->cancelled && *context->cancelled && (*context->cancelled)()
        ? 1
        : 0;
}

inline std::int32_t studio_print_wait(
    void* opaque,
    std::int32_t state,
    const std::uint8_t* body,
    std::size_t body_len
) {
    auto* context = static_cast<StudioPrintCallbackContext*>(opaque);
    if (!context || !context->wait || !*context->wait) return 1;
    std::string information;
    if (body && body_len > 0) {
        information.assign(reinterpret_cast<const char*>(body), body_len);
    }
    return (*context->wait)(state, std::move(information)) ? 1 : 0;
}

inline std::int32_t studio_print_snapshot(
    void* opaque,
    PluginStudioSnapshot* snapshot
) {
    auto* context = static_cast<StudioPrintCallbackContext*>(opaque);
    if (!context || !context->agent || !context->dev_id || !snapshot) return 0;
    context->current_snapshot = printer_request_snapshot(context->agent, *context->dev_id);
    *snapshot = plugin_studio_snapshot(context->current_snapshot);
    return 1;
}

inline PluginStudioPrintParams studio_print_params(
    const PrinterRequestSnapshot& snapshot,
    const BBL::PrintParams& params
) {
    return {
        plugin_studio_snapshot(snapshot),
        plugin_bytes(params.dev_id),
        plugin_bytes(params.task_name),
        plugin_bytes(params.project_name),
        plugin_bytes(params.preset_name),
        plugin_bytes(params.filename),
        plugin_bytes(params.config_filename),
        params.plate_index,
        plugin_bytes(params.ftp_folder),
        plugin_bytes(params.ftp_file),
        plugin_bytes(params.ftp_file_md5),
        plugin_bytes(params.nozzle_mapping),
        plugin_bytes(params.ams_mapping),
        plugin_bytes(params.ams_mapping2),
        plugin_bytes(params.ams_mapping_info),
        plugin_bytes(params.nozzles_info),
        plugin_bytes(params.connection_type),
        plugin_bytes(params.comments),
        params.origin_profile_id,
        params.stl_design_id,
        plugin_bytes(params.origin_model_id),
        plugin_bytes(params.print_type),
        plugin_bytes(params.dst_file),
        plugin_bytes(params.dev_name),
        plugin_bytes(params.dev_ip),
        static_cast<std::uint8_t>(params.use_ssl_for_ftp),
        static_cast<std::uint8_t>(params.use_ssl_for_mqtt),
        plugin_bytes(params.username),
        plugin_bytes(params.password),
        static_cast<std::uint8_t>(params.task_bed_leveling),
        static_cast<std::uint8_t>(params.task_flow_cali),
        static_cast<std::uint8_t>(params.task_vibration_cali),
        static_cast<std::uint8_t>(params.task_layer_inspect),
        static_cast<std::uint8_t>(params.task_record_timelapse),
        static_cast<std::uint8_t>(params.task_timelapse_use_internal),
        static_cast<std::uint8_t>(params.task_use_ams),
        plugin_bytes(params.task_bed_type),
        plugin_bytes(params.extra_options),
        params.auto_bed_leveling,
        params.auto_flow_cali,
        params.auto_offset_cali,
        params.extruder_cali_manual_mode,
        static_cast<std::uint8_t>(params.task_ext_change_assist),
        static_cast<std::uint8_t>(params.try_emmc_print),
#if defined(PANDAR_STUDIO_PRINT_SVC_CONTEXT)
        plugin_bytes(params.svc_context),
#else
        {},
#endif
        {},
    };
}

inline int start_studio_print(
    Agent* agent,
    const BBL::PrintParams& params,
    const BBL::OnUpdateStatusFn& update,
    const BBL::WasCancelledFn& cancelled,
    const BBL::OnWaitFn& wait
) {
    const auto snapshot = printer_request_snapshot(agent, params.dev_id);
    const auto print_params = studio_print_params(snapshot, params);
    StudioPrintCallbackContext context{
        agent,
        &params.dev_id,
        {},
        &update,
        &cancelled,
        &wait,
    };
    const PluginStudioCallbacks callbacks{
        &context,
        studio_print_update,
        studio_print_cancelled,
        studio_print_wait,
        studio_print_snapshot,
    };
    return pandar_plugin_studio_start_print(&print_params, callbacks);
}

struct StudioAccountSnapshotContext {
    Agent* agent;
    PrinterRequestSnapshot current_snapshot;
};

inline std::int32_t studio_account_snapshot(
    void* opaque,
    PluginStudioSnapshot* snapshot
) {
    auto* context = static_cast<StudioAccountSnapshotContext*>(opaque);
    if (!context || !context->agent || !snapshot) return 0;
    context->current_snapshot = printer_request_snapshot(context->agent, {});
    *snapshot = plugin_studio_snapshot(context->current_snapshot);
    return 1;
}

inline PluginStudioAccount studio_account(
    const PrinterRequestSnapshot& snapshot,
    StudioAccountSnapshotContext& context
) {
    return {
        plugin_studio_snapshot(snapshot),
        &context,
        studio_account_snapshot,
    };
}

} // namespace pandar::network_plugin
