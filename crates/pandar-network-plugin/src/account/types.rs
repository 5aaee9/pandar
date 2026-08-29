use std::slice;

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};

pub(super) fn borrowed<'a>(ptr: *const u8, len: usize) -> anyhow::Result<&'a str> {
    ensure!(!ptr.is_null() || len == 0, "account input pointer is null");
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(ptr, len) }
    };
    std::str::from_utf8(bytes).context("account input is not UTF-8")
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[repr(i32)]
#[serde(rename_all = "snake_case")]
pub(super) enum SessionKind {
    Authenticated = 1,
    NoAuth = 2,
}

impl TryFrom<i32> for SessionKind {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Authenticated),
            2 => Ok(Self::NoAuth),
            _ => anyhow::bail!("invalid account session kind"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct ProfileInput {
    #[serde(default)]
    pub(super) token: String,
    #[serde(default, rename = "uidStr")]
    uid_str: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    tenant_id: String,
    #[serde(default)]
    tenant_name: String,
    #[serde(default)]
    avatar: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct Profile {
    pub(super) user_id: String,
    pub(super) user_name: String,
    pub(super) tenant_id: String,
    pub(super) tenant_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) avatar: String,
}

impl ProfileInput {
    pub(super) fn normalize(self) -> anyhow::Result<Profile> {
        let user_id = first_nonempty([self.user_id, self.uid_str]);
        let user_name = first_nonempty([
            self.user_name,
            self.name,
            self.tenant_name.clone(),
            user_id.clone(),
        ]);
        ensure!(!user_id.is_empty(), "account profile has no user id");
        Ok(Profile {
            user_id,
            user_name,
            tenant_id: self.tenant_id,
            tenant_name: self.tenant_name,
            avatar: self.avatar,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AccountChangeInput {
    Studio(StudioLoginInput),
    Profile(ProfileInput),
}

pub(super) enum AccountChange {
    Login { token: String, profile: Profile },
    ConfirmCurrent(Profile),
}

#[derive(Debug, Deserialize)]
struct StudioLoginInput {
    data: StudioLoginData,
}

#[derive(Debug, Deserialize)]
struct StudioLoginData {
    token: String,
    user: StudioLoginUser,
}

#[derive(Debug, Deserialize)]
struct StudioLoginUser {
    #[serde(default, alias = "uidStr", alias = "user_id")]
    uid: String,
    #[serde(default, alias = "user_name")]
    name: String,
    #[serde(default)]
    account: String,
    #[serde(default)]
    avatar: String,
}

pub(super) fn parse_account_change(value: &str) -> anyhow::Result<AccountChange> {
    match serde_json::from_str::<AccountChangeInput>(value)
        .context("decode typed Studio account change")?
    {
        AccountChangeInput::Studio(input) => {
            let token = input.data.token;
            ensure!(!token.trim().is_empty(), "account profile has no token");
            let user_id = input.data.user.uid;
            ensure!(!user_id.trim().is_empty(), "account profile has no user id");
            let user_name = first_nonempty([
                input.data.user.name,
                input.data.user.account,
                user_id.clone(),
            ]);
            Ok(AccountChange::Login {
                token,
                profile: Profile {
                    user_id,
                    user_name,
                    tenant_id: String::new(),
                    tenant_name: String::new(),
                    avatar: input.data.user.avatar,
                },
            })
        }
        AccountChangeInput::Profile(input) => {
            let token = input.token.clone();
            let profile = input.normalize()?;
            if token.trim().is_empty() {
                Ok(AccountChange::ConfirmCurrent(profile))
            } else {
                Ok(AccountChange::Login { token, profile })
            }
        }
    }
}

fn first_nonempty<const N: usize>(values: [String; N]) -> String {
    values
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub(super) struct SessionInput {
    pub(super) token: String,
    pub(super) profile: ProfileInput,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct PersistedLogin {
    pub(super) hub_url: String,
    pub(super) token: String,
    pub(super) session_kind: SessionKind,
    pub(super) profile: Profile,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct PendingRevocation {
    pub(super) hub_url: String,
    pub(super) token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioToken<'a> {
    pub(super) access_token: &'a str,
    pub(super) refresh_token: &'static str,
    pub(super) expires_in: u32,
    pub(super) refresh_expires_in: u32,
    pub(super) tfa_key: &'static str,
    pub(super) access_method: &'static str,
    pub(super) login_type: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StudioProfile<'a> {
    #[serde(rename = "uidStr")]
    pub(super) user_id: &'a str,
    pub(super) account: &'a str,
    pub(super) name: &'a str,
    pub(super) avatar: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct LoginEnvelope<'a> {
    pub(super) sequence_id: &'static str,
    pub(super) command: &'static str,
    pub(super) data: LoginEnvelopeData<'a>,
}

#[derive(Debug, Serialize)]
pub(super) struct LoginEnvelopeData<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) avatar: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) user_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) user_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nickname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) account: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) refresh: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalServerBaseUrl {
    pub(super) base_url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalServerConfig {
    pub(super) hub_url: String,
}

pub(super) fn parse_profile(value: &str) -> anyhow::Result<Profile> {
    serde_json::from_str::<ProfileInput>(value)
        .context("decode typed account profile")?
        .normalize()
}
