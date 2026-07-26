#pragma once

#include "shim_file_transfer_types.hpp"
#include "shim_account_ffi.hpp"

using namespace Slic3r;

namespace {

struct Tunnel {
    std::atomic<int> refs{1};
    ft_tunnel_status_cb status_cb = nullptr;
    void* status_user = nullptr;
    bool closed = false;
};

struct Job {
    std::atomic<int> refs{1};
    ft_job_result_cb result_cb = nullptr;
    void* result_user = nullptr;
    ft_job_msg_cb msg_cb = nullptr;
    void* msg_user = nullptr;
    bool cancelled = false;
    bool finished = false;
    ft_job_result result{};
    std::mutex mutex;
    std::condition_variable cv;
};

void retain(Tunnel* tunnel) {
    if (tunnel) tunnel->refs.fetch_add(1, std::memory_order_relaxed);
}

void release(Tunnel* tunnel) {
    if (tunnel && tunnel->refs.fetch_sub(1, std::memory_order_acq_rel) == 1) delete tunnel;
}

void retain(Job* job) {
    if (job) job->refs.fetch_add(1, std::memory_order_relaxed);
}

void release(Job* job) {
    if (job && job->refs.fetch_sub(1, std::memory_order_acq_rel) == 1) delete job;
}

}

PANDAR_ABI int ft_abi_version() { return 1; }
PANDAR_ABI void ft_free(void*) {}
PANDAR_ABI void ft_job_result_destroy(ft_job_result*) {}
PANDAR_ABI void ft_job_msg_destroy(ft_job_msg*) {}

PANDAR_ABI ft_err ft_tunnel_create(const char*, FT_TunnelHandle** out) {
    if (!out) return FT_EINVAL;
    *out = reinterpret_cast<FT_TunnelHandle*>(new Tunnel());
    return FT_OK;
}

PANDAR_ABI void ft_tunnel_retain(FT_TunnelHandle* h) { retain(reinterpret_cast<Tunnel*>(h)); }
PANDAR_ABI void ft_tunnel_release(FT_TunnelHandle* h) { release(reinterpret_cast<Tunnel*>(h)); }

PANDAR_ABI ft_err ft_tunnel_start_connect(FT_TunnelHandle* h, ft_tunnel_connect_cb cb, void* user) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    retain(tunnel);
    auto status_cb = tunnel->status_cb;
    auto* status_user = tunnel->status_user;
    auto result = pandar::network_plugin::pandar_plugin_studio_file_transfer_unavailable();
    std::string message;
    if (result.body_ptr && result.body_len > 0) {
        message.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
    }
    pandar::network_plugin::pandar_plugin_free_with_capacity(
        result.body_ptr, result.body_len, result.body_cap
    );
    if (cb) cb(user, 1, FT_EIO, message.c_str());
    if (status_cb) {
        status_cb(status_user, 0, -1, FT_EIO, message.c_str());
    }
    release(tunnel);
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_sync_connect(FT_TunnelHandle* h) {
    return h ? FT_EIO : FT_EINVAL;
}

PANDAR_ABI ft_err ft_tunnel_set_status_cb(FT_TunnelHandle* h, ft_tunnel_status_cb cb, void* user) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    tunnel->status_cb = cb;
    tunnel->status_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_shutdown(FT_TunnelHandle* h) {
    auto* tunnel = reinterpret_cast<Tunnel*>(h);
    if (!tunnel) return FT_EINVAL;
    tunnel->closed = true;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_create(const char*, FT_JobHandle** out) {
    if (!out) return FT_EINVAL;
    *out = reinterpret_cast<FT_JobHandle*>(new Job());
    return FT_OK;
}

PANDAR_ABI void ft_job_retain(FT_JobHandle* h) { retain(reinterpret_cast<Job*>(h)); }
PANDAR_ABI void ft_job_release(FT_JobHandle* h) { release(reinterpret_cast<Job*>(h)); }

PANDAR_ABI ft_err ft_job_set_result_cb(FT_JobHandle* h, ft_job_result_cb cb, void* user) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->result_cb = cb;
    job->result_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_get_result(FT_JobHandle* h, uint32_t timeout_ms, ft_job_result* out) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job || !out) return FT_EINVAL;
    std::unique_lock<std::mutex> lock(job->mutex);
    if (!job->finished) {
        job->cv.wait_for(lock, std::chrono::milliseconds(timeout_ms), [job] { return job->finished; });
    }
    *out = job->finished ? job->result : ft_job_result{FT_ETIMEOUT, 0, nullptr, nullptr, 0};
    return FT_OK;
}

PANDAR_ABI ft_err ft_tunnel_start_job(FT_TunnelHandle* th, FT_JobHandle* jh) {
    if (!th || !jh) return FT_EINVAL;
    auto* job = reinterpret_cast<Job*>(jh);
    {
        std::lock_guard<std::mutex> lock(job->mutex);
        job->result = ft_job_result{FT_EIO, 0, nullptr, nullptr, 0};
        job->finished = true;
    }
    job->cv.notify_all();
    if (job->result_cb) job->result_cb(job->result_user, job->result);
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_cancel(FT_JobHandle* h) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->cancelled = true;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_set_msg_cb(FT_JobHandle* h, ft_job_msg_cb cb, void* user) {
    auto* job = reinterpret_cast<Job*>(h);
    if (!job) return FT_EINVAL;
    job->msg_cb = cb;
    job->msg_user = user;
    return FT_OK;
}

PANDAR_ABI ft_err ft_job_try_get_msg(FT_JobHandle* h, ft_job_msg* out) {
    if (out) *out = ft_job_msg{};
    return h ? FT_EIO : FT_EINVAL;
}

PANDAR_ABI ft_err ft_job_get_msg(FT_JobHandle* h, uint32_t, ft_job_msg* out) {
    if (out) *out = ft_job_msg{};
    return h ? FT_EIO : FT_EINVAL;
}
