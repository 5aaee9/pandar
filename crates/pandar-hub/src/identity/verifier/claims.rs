use std::collections::HashSet;

use serde::Deserialize;

use crate::identity::ExternalAuthConfig;

use super::JwtVerifyError;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct JwtClaims {
    pub(super) iss: String,
    pub(super) sub: String,
    pub(super) exp: u64,
    pub(super) iat: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) nbf: Option<u64>,
    #[serde(default)]
    pub(super) aud: Option<AudienceClaim>,
    #[serde(default)]
    pub(super) azp: Option<String>,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) scp: Vec<String>,
    #[serde(default)]
    pub(super) email: Option<String>,
    #[serde(default)]
    pub(super) email_verified: Option<bool>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) preferred_username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(crate) enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

impl AudienceClaim {
    fn values(&self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value.clone()],
            Self::Many(values) => values.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExternalIdentity {
    pub provider: String,
    pub subject: String,
    pub issuer: String,
    pub audiences: Vec<String>,
    pub authorized_party: Option<String>,
    pub scopes: Vec<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
}

impl VerifiedExternalIdentity {
    pub fn verified_email(&self) -> Option<&str> {
        match (self.email.as_deref(), self.email_verified) {
            (Some(email), Some(true)) if !email.trim().is_empty() => Some(email.trim()),
            _ => None,
        }
    }

    pub fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.preferred_username
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| self.verified_email())
            .unwrap_or("")
            .to_owned()
    }
}

pub(super) fn verified_identity(
    config: &ExternalAuthConfig,
    claims: JwtClaims,
) -> Result<VerifiedExternalIdentity, JwtVerifyError> {
    let subject = claims.sub.trim();
    if subject.is_empty() {
        return Err(JwtVerifyError::MissingSubject);
    }

    if !config.authorized_parties.is_empty() {
        let authorized_party = claims
            .azp
            .as_deref()
            .ok_or(JwtVerifyError::UnauthorizedParty)?;
        if !config
            .authorized_parties
            .iter()
            .any(|allowed| allowed == authorized_party)
        {
            return Err(JwtVerifyError::UnauthorizedParty);
        }
    }

    let scopes = scopes_from_claims(&claims);
    if !config.required_scopes.is_empty() {
        let scope_set = scopes.iter().map(String::as_str).collect::<HashSet<_>>();
        if !config
            .required_scopes
            .iter()
            .all(|scope| scope_set.contains(scope.as_str()))
        {
            return Err(JwtVerifyError::MissingScope);
        }
    }

    Ok(VerifiedExternalIdentity {
        provider: config.provider.clone(),
        subject: subject.to_owned(),
        issuer: claims.iss,
        audiences: claims
            .aud
            .map(|audience| audience.values())
            .unwrap_or_default(),
        authorized_party: claims.azp,
        scopes,
        email: claims.email,
        email_verified: claims.email_verified,
        name: claims.name,
        preferred_username: claims.preferred_username,
    })
}

fn scopes_from_claims(claims: &JwtClaims) -> Vec<String> {
    let mut scopes = claims
        .scope
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    scopes.extend(claims.scp.iter().cloned());
    scopes
}
