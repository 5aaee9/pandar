pub(crate) mod lifecycle;
mod persistence;
mod revocation;
mod runtime;
mod session;
mod types;

#[cfg(test)]
mod tests;

use anyhow::{Context, ensure};

use crate::{PluginHttpResult, result, stable_error_body};
use runtime::canonical_hub_identity;
#[cfg(test)]
use runtime::pandar_plugin_account_debug_consistent;
use types::{
    LocalServerBaseUrl, LocalServerConfig, LoginEnvelope, LoginEnvelopeData, PersistedLogin,
    Profile, SessionInput, SessionKind, StudioProfile, StudioToken, borrowed, parse_profile,
};

type AccountVisitor = unsafe extern "C" fn(
    *mut std::ffi::c_void,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    i32,
);
const ACCOUNT_FAILURE: &str = "account_state_unavailable";

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_decode_session(
    body_ptr: *const u8,
    body_len: usize,
    session_kind: i32,
    context: *mut std::ffi::c_void,
    visitor: Option<AccountVisitor>,
) -> PluginHttpResult {
    account_result((|| {
        let body = borrowed(body_ptr, body_len)?;
        let session: SessionInput =
            serde_json::from_str(body).context("decode typed account session")?;
        ensure!(
            !session.token.trim().is_empty(),
            "account session has no token"
        );
        let token = session.token;
        let profile = session.profile.normalize()?;
        visit_account(
            context,
            visitor,
            &token,
            &profile,
            SessionKind::try_from(session_kind)?,
        )?;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_decode_profile(
    body_ptr: *const u8,
    body_len: usize,
    fallback_token_ptr: *const u8,
    fallback_token_len: usize,
    context: *mut std::ffi::c_void,
    visitor: Option<AccountVisitor>,
) -> PluginHttpResult {
    account_result((|| {
        let body = borrowed(body_ptr, body_len)?;
        let input: types::ProfileInput =
            serde_json::from_str(body).context("decode typed account profile input")?;
        let token = if input.token.trim().is_empty() {
            borrowed(fallback_token_ptr, fallback_token_len)?.to_owned()
        } else {
            input.token.clone()
        };
        ensure!(!token.trim().is_empty(), "account profile has no token");
        let profile = input.normalize()?;
        visit_account(
            context,
            visitor,
            &token,
            &profile,
            SessionKind::Authenticated,
        )?;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_load(
    config_dir_ptr: *const u8,
    config_dir_len: usize,
    expected_hub_ptr: *const u8,
    expected_hub_len: usize,
    context: *mut std::ffi::c_void,
    visitor: Option<AccountVisitor>,
) -> PluginHttpResult {
    let loaded = (|| {
        let config_dir = borrowed(config_dir_ptr, config_dir_len)?;
        let expected_hub = borrowed(expected_hub_ptr, expected_hub_len)?;
        let Some(login) = persistence::load(config_dir)? else {
            return Ok(false);
        };
        if canonical_hub_identity(&login.hub_url) != canonical_hub_identity(expected_hub) {
            return Ok(false);
        }
        ensure!(
            !login.token.trim().is_empty(),
            "persisted Studio login has no token"
        );
        visit_account(
            context,
            visitor,
            &login.token,
            &login.profile,
            login.session_kind,
        )?;
        Ok(true)
    })();
    match loaded {
        Ok(true) => result(0, 200, ""),
        Ok(false) => result(2, 204, ""),
        Err(error) => diagnosed(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_persist(
    config_dir_ptr: *const u8,
    config_dir_len: usize,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    session_kind: i32,
    profile_ptr: *const u8,
    profile_len: usize,
) -> PluginHttpResult {
    account_result((|| {
        let token = borrowed(token_ptr, token_len)?;
        let profile_json = borrowed(profile_ptr, profile_len)?;
        if token.is_empty() && profile_json.is_empty() {
            return Ok(());
        }
        let config_dir = borrowed(config_dir_ptr, config_dir_len)?;
        let hub_url = canonical_hub_identity(borrowed(hub_url_ptr, hub_url_len)?);
        ensure!(
            !token.trim().is_empty(),
            "persisted Studio login has no token"
        );
        let profile = parse_profile(profile_json)?;
        persistence::store(
            config_dir,
            &PersistedLogin {
                hub_url,
                token: token.to_owned(),
                session_kind: SessionKind::try_from(session_kind)?,
                profile,
            },
        )?
        .require_confirmed("durably persist Studio login")?;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_clear(
    config_dir_ptr: *const u8,
    config_dir_len: usize,
) -> PluginHttpResult {
    account_result((|| {
        persistence::clear(borrowed(config_dir_ptr, config_dir_len)?)?
            .require_confirmed("durably clear persisted Studio login")?;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_login_envelope(
    logout: bool,
    token_ptr: *const u8,
    token_len: usize,
    user_id_ptr: *const u8,
    user_id_len: usize,
    user_name_ptr: *const u8,
    user_name_len: usize,
    avatar_ptr: *const u8,
    avatar_len: usize,
) -> PluginHttpResult {
    json_result((|| {
        let token = borrowed(token_ptr, token_len)?;
        let user_id = borrowed(user_id_ptr, user_id_len)?;
        let user_name = borrowed(user_name_ptr, user_name_len)?;
        let avatar = borrowed(avatar_ptr, avatar_len)?;
        let online = !logout && !token.is_empty();
        serde_json::to_string(&LoginEnvelope {
            sequence_id: "0",
            command: if online {
                "studio_userlogin"
            } else {
                "studio_useroffline"
            },
            data: LoginEnvelopeData {
                avatar: online.then_some(avatar),
                name: online.then_some(user_name),
                user_id: online.then_some(user_id),
                user_name: online.then_some(user_name),
                nickname: online.then_some(user_name),
                account: online.then_some(user_name),
                token: online.then_some(token),
                refresh: online.then_some(""),
            },
        })
        .context("encode Studio login envelope")
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_token_body(
    token_ptr: *const u8,
    token_len: usize,
) -> PluginHttpResult {
    json_result((|| {
        let token = borrowed(token_ptr, token_len)?;
        serde_json::to_string(&StudioToken {
            access_token: token,
            refresh_token: "",
            expires_in: 31_536_000,
            refresh_expires_in: 31_536_000,
            tfa_key: "",
            access_method: "pandar",
            login_type: "pandar",
        })
        .context("encode Studio token body")
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_profile_body(
    user_id_ptr: *const u8,
    user_id_len: usize,
    user_name_ptr: *const u8,
    user_name_len: usize,
    avatar_ptr: *const u8,
    avatar_len: usize,
) -> PluginHttpResult {
    let body = (|| {
        let user_id = borrowed(user_id_ptr, user_id_len)?;
        let user_name = borrowed(user_name_ptr, user_name_len)?;
        if user_id.is_empty() || user_name.is_empty() {
            return Ok(None);
        }
        serde_json::to_string(&StudioProfile {
            user_id,
            account: user_name,
            name: user_name,
            avatar: borrowed(avatar_ptr, avatar_len)?,
        })
        .context("encode Studio profile body")
        .map(Some)
    })();
    match body {
        Ok(Some(body)) => result(0, 200, body),
        Ok(None) => result(-19, 401, stable_error_body("profile_unavailable")),
        Err(error) => diagnosed(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_local_base_url(
    body_ptr: *const u8,
    body_len: usize,
) -> PluginHttpResult {
    json_field_result::<LocalServerBaseUrl>(body_ptr, body_len, |value| value.base_url)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_local_hub_url(
    body_ptr: *const u8,
    body_len: usize,
) -> PluginHttpResult {
    json_field_result::<LocalServerConfig>(body_ptr, body_len, |value| {
        canonical_hub_identity(&value.hub_url)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_session_action(status: i32, http_code: u32) -> i32 {
    if status != 0 && matches!(http_code, 401 | 410) {
        1
    } else {
        0
    }
}

fn visit_account(
    context: *mut std::ffi::c_void,
    visitor: Option<AccountVisitor>,
    token: &str,
    profile: &Profile,
    session_kind: SessionKind,
) -> anyhow::Result<()> {
    let visitor = visitor.context("account visitor is missing")?;
    let profile_json =
        serde_json::to_string(profile).context("encode canonical account profile")?;
    unsafe {
        visitor(
            context,
            token.as_ptr(),
            token.len(),
            profile.user_id.as_ptr(),
            profile.user_id.len(),
            profile.user_name.as_ptr(),
            profile.user_name.len(),
            profile.avatar.as_ptr(),
            profile.avatar.len(),
            profile_json.as_ptr(),
            profile_json.len(),
            session_kind as i32,
        );
    }
    Ok(())
}

fn json_field_result<T: serde::de::DeserializeOwned>(
    body_ptr: *const u8,
    body_len: usize,
    select: impl FnOnce(T) -> String,
) -> PluginHttpResult {
    json_result((|| {
        let value = serde_json::from_str::<T>(borrowed(body_ptr, body_len)?)
            .context("decode typed local server response")?;
        let selected = select(value);
        ensure!(!selected.is_empty(), "local server response field is empty");
        Ok(selected)
    })())
}

fn account_result(work: anyhow::Result<()>) -> PluginHttpResult {
    match work {
        Ok(()) => result(0, 200, ""),
        Err(error) => diagnosed(error),
    }
}

fn json_result(work: anyhow::Result<String>) -> PluginHttpResult {
    match work {
        Ok(body) => result(0, 200, body),
        Err(error) => diagnosed(error),
    }
}

fn diagnosed(error: anyhow::Error) -> PluginHttpResult {
    eprintln!("pandar network plugin account state failed: {error:#}");
    result(1, 0, stable_error_body(ACCOUNT_FAILURE))
}
