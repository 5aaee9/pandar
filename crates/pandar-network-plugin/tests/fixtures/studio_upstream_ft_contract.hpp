#pragma once

struct FtCallbacks {
    static constexpr std::uint64_t canary = UINT64_C(0x51a7c0de8badf00d);
    static constexpr std::uint64_t cookie = UINT64_C(0xc001d00d7a11bacc);
    std::uint64_t before{canary};
    std::uint64_t identity{cookie};
    unsigned connect_calls{};
    unsigned status_calls{};
    unsigned result_calls{};
    unsigned message_calls{};
    bool connect_valid{};
    bool status_valid{};
    bool result_valid{};
    std::uint64_t after{canary};

    void verify_canaries() const
    {
        if (before != canary || identity != cookie || after != canary) {
            fail("FT callback payload canary or identity cookie was corrupted");
        }
    }
};

bool exact_result(const Slic3r::ft_job_result& result, int error)
{
    return result.ec == error && result.resp_ec == 0 && !result.json && !result.bin &&
        result.bin_size == 0;
}

bool empty_message(const Slic3r::ft_job_msg& message)
{
    return message.kind == 0 && !message.json;
}

void FT_CALL on_connect(void* user, int ok, int error, const char* message)
{
    auto* callbacks = static_cast<FtCallbacks*>(user);
    callbacks->verify_canaries();
    ++callbacks->connect_calls;
    callbacks->connect_valid = ok == 1 && error == Slic3r::FT_EIO && message &&
        std::string(message) == R"({"error":"unsupported_file_transfer"})";
    callbacks->verify_canaries();
}

void FT_CALL on_status(void* user, int old_status, int new_status, int error, const char* message)
{
    auto* callbacks = static_cast<FtCallbacks*>(user);
    callbacks->verify_canaries();
    ++callbacks->status_calls;
    callbacks->status_valid = old_status == 0 && new_status == -1 &&
        error == Slic3r::FT_EIO && message &&
        std::string(message) == R"({"error":"unsupported_file_transfer"})";
    callbacks->verify_canaries();
}

void FT_CALL on_result(void* user, Slic3r::ft_job_result result)
{
    auto* callbacks = static_cast<FtCallbacks*>(user);
    callbacks->verify_canaries();
    ++callbacks->result_calls;
    callbacks->result_valid = exact_result(result, Slic3r::FT_EIO);
    callbacks->verify_canaries();
}

void FT_CALL on_message(void* user, Slic3r::ft_job_msg)
{
    auto* callbacks = static_cast<FtCallbacks*>(user);
    callbacks->verify_canaries();
    ++callbacks->message_calls;
    callbacks->verify_canaries();
}

void check_ft(const Library& library)
{
    const auto abi = library.require<Slic3r::fn_ft_abi_version>("ft_abi_version");
    const auto free_memory = library.require<Slic3r::fn_ft_free>("ft_free");
    const auto destroy_result =
        library.require<Slic3r::fn_ft_job_result_destroy>("ft_job_result_destroy");
    const auto destroy_message =
        library.require<Slic3r::fn_ft_job_msg_destroy>("ft_job_msg_destroy");
    const auto tunnel_create = library.require<Slic3r::fn_ft_tunnel_create>("ft_tunnel_create");
    const auto tunnel_retain = library.require<Slic3r::fn_ft_tunnel_retain>("ft_tunnel_retain");
    const auto tunnel_release = library.require<Slic3r::fn_ft_tunnel_release>("ft_tunnel_release");
    const auto tunnel_connect =
        library.require<Slic3r::fn_ft_tunnel_start_connect>("ft_tunnel_start_connect");
    const auto tunnel_sync =
        library.require<Slic3r::fn_ft_tunnel_sync_connect>("ft_tunnel_sync_connect");
    const auto tunnel_status =
        library.require<Slic3r::fn_ft_tunnel_set_status_cb>("ft_tunnel_set_status_cb");
    const auto tunnel_shutdown =
        library.require<Slic3r::fn_ft_tunnel_shutdown>("ft_tunnel_shutdown");
    const auto job_create = library.require<Slic3r::fn_ft_job_create>("ft_job_create");
    const auto job_retain = library.require<Slic3r::fn_ft_job_retain>("ft_job_retain");
    const auto job_release = library.require<Slic3r::fn_ft_job_release>("ft_job_release");
    const auto job_result_callback =
        library.require<Slic3r::fn_ft_job_set_result_cb>("ft_job_set_result_cb");
    const auto job_result = library.require<Slic3r::fn_ft_job_get_result>("ft_job_get_result");
    const auto tunnel_start_job =
        library.require<Slic3r::fn_ft_tunnel_start_job>("ft_tunnel_start_job");
    const auto job_cancel = library.require<Slic3r::fn_ft_job_cancel>("ft_job_cancel");
    const auto job_message_callback =
        library.require<Slic3r::fn_ft_job_set_msg_cb>("ft_job_set_msg_cb");
    const auto job_try_message =
        library.require<Slic3r::fn_ft_job_try_get_msg>("ft_job_try_get_msg");
    const auto job_message = library.require<Slic3r::fn_ft_job_get_msg>("ft_job_get_msg");

    if (abi() != 1) fail("unsupported ft_abi_version");
    free_memory(nullptr);
    Slic3r::ft_job_result empty_result{};
    Slic3r::ft_job_msg empty_job_message{};
    destroy_result(&empty_result);
    destroy_message(&empty_job_message);
    if (!exact_result(empty_result, 0) || !empty_message(empty_job_message)) {
        fail("FT destroy functions changed empty caller-owned values");
    }

    for (unsigned iteration = 0; iteration < 256; ++iteration) {
        Slic3r::FT_TunnelHandle* tunnel{};
        if (tunnel_create("unsupported://contract", &tunnel) != Slic3r::FT_OK || !tunnel) {
            fail("ft_tunnel_create did not create a safe unsupported handle");
        }
        tunnel_retain(tunnel);
        tunnel_release(tunnel);
        FtCallbacks callbacks{};
        if (tunnel_status(tunnel, on_status, &callbacks) != Slic3r::FT_OK) {
            fail("ft_tunnel_set_status_cb failed");
        }
        if (tunnel_connect(tunnel, on_connect, &callbacks) != Slic3r::FT_OK) {
            fail("ft_tunnel_start_connect failed");
        }
        if (tunnel_sync(tunnel) != Slic3r::FT_EIO) {
            fail("FT unsupported connection result is unstable");
        }

        Slic3r::FT_JobHandle* job{};
        if (job_create("{}", &job) != Slic3r::FT_OK || !job) fail("ft_job_create failed");
        job_retain(job);
        job_release(job);
        if (job_result_callback(job, on_result, &callbacks) != Slic3r::FT_OK) {
            fail("ft_job_set_result_cb failed");
        }
        if (job_message_callback(job, on_message, &callbacks) != Slic3r::FT_OK) {
            fail("ft_job_set_msg_cb failed");
        }
        Slic3r::ft_job_result result{};
        if (job_result(job, 0, &result) != Slic3r::FT_OK ||
            !exact_result(result, Slic3r::FT_ETIMEOUT)) {
            fail("ft_job_get_result did not report timeout before start");
        }
        if (tunnel_start_job(tunnel, job) != Slic3r::FT_OK) {
            fail("ft_tunnel_start_job unsupported result is unstable");
        }
        if (job_result(job, 0, &result) != Slic3r::FT_OK ||
            !exact_result(result, Slic3r::FT_EIO)) {
            fail("ft_job_get_result did not preserve unsupported result");
        }
        Slic3r::ft_job_msg message{7, "dirty"};
        if (job_try_message(job, &message) != Slic3r::FT_EIO || !empty_message(message)) {
            fail("ft_job_try_get_msg result changed");
        }
        message = Slic3r::ft_job_msg{7, "dirty"};
        if (job_message(job, 0, &message) != Slic3r::FT_EIO || !empty_message(message)) {
            fail("ft_job_get_msg result changed");
        }
        if (job_cancel(job) != Slic3r::FT_OK) fail("ft_job_cancel failed");
        if (tunnel_shutdown(tunnel) != Slic3r::FT_OK) fail("ft_tunnel_shutdown failed");
        job_release(job);
        tunnel_release(tunnel);
        callbacks.verify_canaries();
        if (callbacks.connect_calls != 1 || callbacks.status_calls != 1 ||
            callbacks.result_calls != 1 || callbacks.message_calls != 0 ||
            !callbacks.connect_valid || !callbacks.status_valid || !callbacks.result_valid) {
            fail("FT callbacks crossed the pinned typedef with unexpected payload or cardinality");
        }
    }
}
