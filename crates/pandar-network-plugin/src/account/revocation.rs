use anyhow::Context;

use crate::{
    PluginHttpResult, RequestKind, cancellation::RequestCancellation, http, normalize_hub_url,
    result,
};

use super::{borrowed, diagnosed, persistence, types::PendingRevocation};

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_revoke_pending(
    config_dir_ptr: *const u8,
    config_dir_len: usize,
) -> PluginHttpResult {
    let work = (|| {
        let config_dir = unsafe { borrowed(config_dir_ptr, config_dir_len) }?;
        revoke_next(config_dir)
    })();
    revocation_result(work)
}

pub(super) fn revoke_all_with_cancellation(
    config_dir: &str,
    cancellation: RequestCancellation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    loop {
        let Some(revocation) = persistence::load_next_revocation(config_dir)? else {
            return Ok(None);
        };
        let result = revoke_loaded(config_dir, revocation, cancellation)?;
        if result.is_some() {
            return Ok(result);
        }
    }
}

fn revoke_next(config_dir: &str) -> anyhow::Result<Option<PluginHttpResult>> {
    let Some(revocation) = persistence::load_next_revocation(config_dir)? else {
        return Ok(None);
    };
    revoke_loaded(config_dir, revocation, RequestCancellation::disabled())
}

fn revoke_loaded(
    config_dir: &str,
    revocation: persistence::PersistedRevocation,
    cancellation: RequestCancellation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    match revocation {
        persistence::PersistedRevocation::Direct(revocation) => {
            revoke_direct(config_dir, revocation, cancellation)
        }
        persistence::PersistedRevocation::Pending(revocation) => {
            revoke_with_cancellation(config_dir, revocation, cancellation)
        }
    }
}

pub(super) fn revoke(
    config_dir: &str,
    revocation: PendingRevocation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    revoke_with_cancellation(config_dir, revocation, RequestCancellation::disabled())
}

fn revoke_with_cancellation(
    config_dir: &str,
    revocation: PendingRevocation,
    cancellation: RequestCancellation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    persistence::clear_matching(config_dir, &revocation)
        .context("clear matching Studio login before plugin revocation")?
        .report("pandar staged-revocation login removal durability warning");
    persistence::confirm(config_dir)
        .context("confirm pending revocation and login removal before plugin revocation")?;
    let url = revocation_url(&revocation)?;
    let response = http::cancellable::delete_session(
        &url,
        &revocation.token,
        RequestKind::PluginSession,
        cancellation,
    );
    if response.status == 0 || matches!(response.http_code, 401 | 410) {
        unsafe {
            crate::pandar_plugin_free_with_capacity(
                response.body_ptr.cast(),
                response.body_len,
                response.body_cap,
            )
        };
        persistence::complete_pending(config_dir, &revocation)
            .context("complete pending plugin revocation")?;
        Ok(None)
    } else {
        Ok(Some(response))
    }
}

pub(super) fn revoke_orphan(
    config_dir: &str,
    revocation: PendingRevocation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    match persistence::prepare_orphan_direct(config_dir, &revocation) {
        Ok(persistence::MutationDurability::Confirmed) => {
            revoke_direct(config_dir, revocation, RequestCancellation::disabled())
        }
        Ok(persistence::MutationDurability::ChangedUnconfirmed(error)) => {
            Err(error).context("direct orphan revocation intent durability is unconfirmed")
        }
        Err(error) => {
            eprintln!("pandar direct orphan revocation persistence failed: {error:#}");
            revoke_best_effort_orphan(revocation)
        }
    }
}

fn revoke_best_effort_orphan(
    revocation: PendingRevocation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    let url = revocation_url(&revocation)?;
    let response = http::delete_session(&url, &revocation.token, RequestKind::PluginSession);
    if response.status == 0 || matches!(response.http_code, 401 | 410) {
        unsafe {
            crate::pandar_plugin_free_with_capacity(
                response.body_ptr.cast(),
                response.body_len,
                response.body_cap,
            )
        };
        Ok(None)
    } else {
        Ok(Some(response))
    }
}

fn revoke_direct(
    config_dir: &str,
    revocation: PendingRevocation,
    cancellation: RequestCancellation,
) -> anyhow::Result<Option<PluginHttpResult>> {
    let url = revocation_url(&revocation)?;
    let response = http::cancellable::delete_session(
        &url,
        &revocation.token,
        RequestKind::PluginSession,
        cancellation,
    );
    if response.status == 0 || matches!(response.http_code, 401 | 410) {
        unsafe {
            crate::pandar_plugin_free_with_capacity(
                response.body_ptr.cast(),
                response.body_len,
                response.body_cap,
            )
        };
        persistence::complete_direct(config_dir, &revocation)
            .context("complete direct plugin revocation")?;
        Ok(None)
    } else {
        Ok(Some(response))
    }
}

fn revocation_url(revocation: &PendingRevocation) -> anyhow::Result<String> {
    let hub_url = normalize_hub_url(revocation.hub_url.clone())
        .context("pending revocation has an insecure or invalid Hub URL")?;
    Ok(format!("{hub_url}/api/v1/plugin/session"))
}

pub(super) fn revocation_result(
    work: anyhow::Result<Option<PluginHttpResult>>,
) -> PluginHttpResult {
    match work {
        Ok(Some(response)) => response,
        Ok(None) => result(0, 204, ""),
        Err(error) => diagnosed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_url_rejects_remote_cleartext_hub() {
        let revocation = PendingRevocation {
            hub_url: "http://hub.example.test".to_owned(),
            token: "secret".to_owned(),
        };

        assert!(revocation_url(&revocation).is_err());
    }
}
