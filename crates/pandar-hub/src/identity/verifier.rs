use std::{collections::HashSet, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, Jwk, JwkSet, KeyAlgorithm},
};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::RwLock;

use super::ExternalAuthConfig;

#[derive(Debug, Error)]
pub enum JwtVerifyError {
    #[error("invalid jwt header")]
    InvalidHeader(#[source] jsonwebtoken::errors::Error),
    #[error("missing jwt key id")]
    MissingKeyId,
    #[error("unsupported jwt algorithm")]
    UnsupportedAlgorithm,
    #[error("failed to load jwks")]
    Jwks(#[source] anyhow::Error),
    #[error("unknown jwt key id")]
    UnknownKeyId,
    #[error("unsupported jwk")]
    UnsupportedJwk,
    #[error("jwk algorithm mismatch")]
    JwkAlgorithmMismatch,
    #[error("invalid jwt claims")]
    InvalidClaims(#[source] jsonwebtoken::errors::Error),
    #[error("missing jwt subject")]
    MissingSubject,
    #[error("unauthorized jwt authorized party")]
    UnauthorizedParty,
    #[error("missing required jwt scope")]
    MissingScope,
}

#[derive(Debug, Clone, Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    #[allow(dead_code)]
    exp: u64,
    #[serde(default)]
    #[allow(dead_code)]
    nbf: Option<u64>,
    #[serde(default)]
    aud: Option<AudienceClaim>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    scp: Vec<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(super) enum AudienceClaim {
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

#[async_trait]
trait JwksSource: Send + Sync {
    async fn load_jwks(&self) -> anyhow::Result<JwkSet>;
}

#[derive(Debug, Clone)]
struct RemoteJwksSource {
    client: reqwest::Client,
    jwks_url: String,
}

impl RemoteJwksSource {
    fn new(jwks_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            jwks_url,
        }
    }
}

#[async_trait]
impl JwksSource for RemoteJwksSource {
    async fn load_jwks(&self) -> anyhow::Result<JwkSet> {
        self.client
            .get(&self.jwks_url)
            .send()
            .await
            .with_context(|| format!("failed to fetch JWKS from {}", self.jwks_url))?
            .error_for_status()
            .with_context(|| format!("JWKS endpoint returned error for {}", self.jwks_url))?
            .json::<JwkSet>()
            .await
            .with_context(|| format!("failed to decode JWKS from {}", self.jwks_url))
    }
}

#[derive(Clone)]
pub struct JwtVerifier {
    config: ExternalAuthConfig,
    jwks_source: Arc<dyn JwksSource>,
    cache: Arc<RwLock<Option<JwkSet>>>,
}

impl std::fmt::Debug for JwtVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtVerifier")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl JwtVerifier {
    pub fn remote(config: ExternalAuthConfig) -> Self {
        let jwks_source = RemoteJwksSource::new(config.jwks_url.clone());
        Self::new(config, Arc::new(jwks_source))
    }

    #[cfg(test)]
    pub fn static_jwks(config: ExternalAuthConfig, jwks: JwkSet) -> Self {
        Self {
            config,
            jwks_source: Arc::new(StaticJwksSource { jwks: jwks.clone() }),
            cache: Arc::new(RwLock::new(Some(jwks))),
        }
    }

    fn new(config: ExternalAuthConfig, jwks_source: Arc<dyn JwksSource>) -> Self {
        Self {
            config,
            jwks_source,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn verify(&self, token: &str) -> Result<VerifiedExternalIdentity, JwtVerifyError> {
        let header = decode_header(token).map_err(JwtVerifyError::InvalidHeader)?;
        if !self.config.algorithms.contains(&header.alg) {
            return Err(JwtVerifyError::UnsupportedAlgorithm);
        }
        let kid = header.kid.as_deref().ok_or(JwtVerifyError::MissingKeyId)?;

        let jwks = self.cached_or_fetch().await?;
        let jwk = match jwks.find(kid) {
            Some(jwk) => jwk.clone(),
            None => {
                let jwks = self.fetch_and_cache().await?;
                jwks.find(kid)
                    .cloned()
                    .ok_or(JwtVerifyError::UnknownKeyId)?
            }
        };

        validate_jwk(&jwk, header.alg)?;
        let key = DecodingKey::from_jwk(&jwk).map_err(|_| JwtVerifyError::UnsupportedJwk)?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = self.config.algorithms.clone();
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.leeway = self.config.leeway_seconds;

        if let Some(audience) = &self.config.audience {
            validation.set_audience(&[audience.as_str()]);
            validation.set_required_spec_claims(&["exp", "iss", "sub", "aud"]);
        } else {
            validation.validate_aud = false;
            validation.set_required_spec_claims(&["exp", "iss", "sub"]);
        }

        let claims = decode::<JwtClaims>(token, &key, &validation)
            .map_err(JwtVerifyError::InvalidClaims)?
            .claims;
        verified_identity(&self.config, claims)
    }

    pub async fn check_ready(&self) -> Result<(), JwtVerifyError> {
        self.cached_or_fetch().await.map(|_| ())
    }

    async fn cached_or_fetch(&self) -> Result<JwkSet, JwtVerifyError> {
        if let Some(jwks) = self.cache.read().await.clone() {
            return Ok(jwks);
        }

        self.fetch_and_cache().await
    }

    async fn fetch_and_cache(&self) -> Result<JwkSet, JwtVerifyError> {
        let jwks = self
            .jwks_source
            .load_jwks()
            .await
            .map_err(JwtVerifyError::Jwks)?;
        *self.cache.write().await = Some(jwks.clone());
        Ok(jwks)
    }
}

fn validate_jwk(jwk: &Jwk, header_algorithm: Algorithm) -> Result<(), JwtVerifyError> {
    if !matches!(jwk.algorithm, AlgorithmParameters::RSA(_)) {
        return Err(JwtVerifyError::UnsupportedJwk);
    }

    if let Some(key_algorithm) = jwk.common.key_algorithm {
        let Some(jwk_algorithm) = key_algorithm_to_algorithm(key_algorithm) else {
            return Err(JwtVerifyError::UnsupportedJwk);
        };
        if jwk_algorithm != header_algorithm {
            return Err(JwtVerifyError::JwkAlgorithmMismatch);
        }
    }

    Ok(())
}

fn key_algorithm_to_algorithm(key_algorithm: KeyAlgorithm) -> Option<Algorithm> {
    match key_algorithm {
        KeyAlgorithm::RS256 => Some(Algorithm::RS256),
        KeyAlgorithm::RS384 => Some(Algorithm::RS384),
        KeyAlgorithm::RS512 => Some(Algorithm::RS512),
        _ => None,
    }
}

fn verified_identity(
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

#[cfg(test)]
#[derive(Debug, Clone)]
struct StaticJwksSource {
    jwks: JwkSet,
}

#[cfg(test)]
#[async_trait]
impl JwksSource for StaticJwksSource {
    async fn load_jwks(&self) -> anyhow::Result<JwkSet> {
        Ok(self.jwks.clone())
    }
}
