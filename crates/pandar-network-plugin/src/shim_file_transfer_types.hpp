#pragma once

#include "shim_types.hpp"

#if defined(_WIN32)
#define PANDAR_FT_CALL __cdecl
#else
#define PANDAR_FT_CALL
#endif

namespace Slic3r {

extern "C" {

struct ft_job_result {
    int ec;
    int resp_ec;
    const char* json;
    const void* bin;
    uint32_t bin_size;
};

struct ft_job_msg {
    int kind;
    const char* json;
};

enum ft_err {
    FT_OK = 0,
    FT_EINVAL = -1,
    FT_ESTATE = -2,
    FT_EIO = -3,
    FT_ETIMEOUT = -4,
    FT_ECANCELLED = -5,
    FT_EXCEPTION = -6,
    FT_EUNKNOWN = -128,
};

struct FT_TunnelHandle;
struct FT_JobHandle;

}

using ft_tunnel_connect_cb =
    void(PANDAR_FT_CALL*)(void* user, int ok, int err, const char* msg);
using ft_tunnel_status_cb = void(PANDAR_FT_CALL*)(
    void* user,
    int old_status,
    int new_status,
    int err,
    const char* msg
);
using ft_job_result_cb = void(PANDAR_FT_CALL*)(void* user, ft_job_result result);
using ft_job_msg_cb = void(PANDAR_FT_CALL*)(void* user, ft_job_msg msg);

} // namespace Slic3r

#undef PANDAR_FT_CALL
