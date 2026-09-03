pub(crate) mod lifecycle;
mod persistence;
mod revocation;
mod runtime;
mod server_selection;
pub(crate) mod session;
mod types;

#[cfg(test)]
mod tests;

use anyhow::{Context, ensure};

use crate::{PluginHttpResult, result, stable_error_body};
#[cfg(test)]
use runtime::pandar_plugin_account_debug_consistent;
use types::{LocalServerBaseUrl, LoginEnvelope, LoginEnvelopeData, borrowed};

const ACCOUNT_FAILURE: &str = "account_state_unavailable";

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_login_envelope(
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
        let token = unsafe { borrowed(token_ptr, token_len) }?;
        let user_id = unsafe { borrowed(user_id_ptr, user_id_len) }?;
        let user_name = unsafe { borrowed(user_name_ptr, user_name_len) }?;
        let avatar = unsafe { borrowed(avatar_ptr, avatar_len) }?;
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
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_account_local_base_url(
    body_ptr: *const u8,
    body_len: usize,
) -> PluginHttpResult {
    unsafe { json_field_result::<LocalServerBaseUrl>(body_ptr, body_len, |value| value.base_url) }
}

unsafe fn json_field_result<T: serde::de::DeserializeOwned>(
    body_ptr: *const u8,
    body_len: usize,
    select: impl FnOnce(T) -> String,
) -> PluginHttpResult {
    json_result((|| {
        let value = serde_json::from_str::<T>(unsafe { borrowed(body_ptr, body_len) }?)
            .context("decode typed local server response")?;
        let selected = select(value);
        ensure!(!selected.is_empty(), "local server response field is empty");
        Ok(selected)
    })())
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
