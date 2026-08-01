#pragma once

static_assert(sizeof(PandarBBL::PrintParams) == sizeof(BBL::PrintParams));
static_assert(alignof(PandarBBL::PrintParams) == alignof(BBL::PrintParams));
static_assert(sizeof(PandarSlic3r::ft_job_result) == sizeof(Slic3r::ft_job_result));
static_assert(alignof(PandarSlic3r::ft_job_result) == alignof(Slic3r::ft_job_result));
static_assert(sizeof(PandarSlic3r::ft_job_msg) == sizeof(Slic3r::ft_job_msg));
static_assert(alignof(PandarSlic3r::ft_job_msg) == alignof(Slic3r::ft_job_msg));
static_assert(sizeof(PandarSlic3r::ft_err) == sizeof(Slic3r::ft_err));
static_assert(alignof(PandarSlic3r::ft_err) == alignof(Slic3r::ft_err));
static_assert(std::is_same_v<
    std::underlying_type_t<PandarSlic3r::ft_err>,
    std::underlying_type_t<Slic3r::ft_err>
>);
static_assert(PandarSlic3r::FT_OK == Slic3r::FT_OK);
static_assert(PandarSlic3r::FT_EINVAL == Slic3r::FT_EINVAL);
static_assert(PandarSlic3r::FT_ESTATE == Slic3r::FT_ESTATE);
static_assert(PandarSlic3r::FT_EIO == Slic3r::FT_EIO);
static_assert(PandarSlic3r::FT_ETIMEOUT == Slic3r::FT_ETIMEOUT);
static_assert(PandarSlic3r::FT_ECANCELLED == Slic3r::FT_ECANCELLED);
static_assert(PandarSlic3r::FT_EXCEPTION == Slic3r::FT_EXCEPTION);
static_assert(PandarSlic3r::FT_EUNKNOWN == Slic3r::FT_EUNKNOWN);

#define PANDAR_CHECK_MEMBER(pandar_type, studio_type, field) \
    static_assert(std::is_same_v< \
        decltype(pandar_type::field), \
        decltype(studio_type::field) \
    >); \
    static_assert(offsetof(pandar_type, field) == offsetof(studio_type, field))

PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, dev_id);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_name);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, project_name);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, preset_name);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, filename);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, config_filename);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, plate_index);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ftp_folder);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ftp_file);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ftp_file_md5);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, nozzle_mapping);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ams_mapping);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ams_mapping2);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, ams_mapping_info);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, nozzles_info);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, connection_type);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, comments);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, origin_profile_id);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, stl_design_id);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, origin_model_id);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, print_type);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, dst_file);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, dev_name);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, dev_ip);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, use_ssl_for_ftp);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, use_ssl_for_mqtt);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, username);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, password);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_bed_leveling);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_flow_cali);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_vibration_cali);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_layer_inspect);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_record_timelapse);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_timelapse_use_internal);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_use_ams);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_bed_type);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, extra_options);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, auto_bed_leveling);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, auto_flow_cali);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, auto_offset_cali);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, extruder_cali_manual_mode);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, task_ext_change_assist);
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, try_emmc_print);
#if defined(PANDAR_STUDIO_PRINT_SVC_CONTEXT)
PANDAR_CHECK_MEMBER(PandarBBL::PrintParams, BBL::PrintParams, svc_context);
#endif

PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_result, Slic3r::ft_job_result, ec);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_result, Slic3r::ft_job_result, resp_ec);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_result, Slic3r::ft_job_result, json);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_result, Slic3r::ft_job_result, bin);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_result, Slic3r::ft_job_result, bin_size);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_msg, Slic3r::ft_job_msg, kind);
PANDAR_CHECK_MEMBER(PandarSlic3r::ft_job_msg, Slic3r::ft_job_msg, json);
#undef PANDAR_CHECK_MEMBER

namespace StudioUpstream = Slic3r;
#define PANDAR_STUDIO_EXPORT(name, target, result, parameters) \
    extern "C" result name parameters; \
    static_assert(std::is_same_v<decltype(&name), StudioUpstream::target>);
#include "shim_exports.hpp"
#undef PANDAR_STUDIO_EXPORT
