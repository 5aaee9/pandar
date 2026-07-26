use std::{sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use futures_util::StreamExt;
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, Jwk, JwkSet, KeyAlgorithm},
};
use thiserror::Error;
use tokio::{
    sync::{Mutex, RwLock},
    time::Instant,
};

use super::ExternalAuthConfig;

mod claims;

#[cfg(test)]
pub(super) use claims::AudienceClaim;
pub use claims::VerifiedExternalIdentity;
use claims::{JwtClaims, verified_identity};

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
    #[error("invalid jwt issued-at or expiration ordering")]
    InvalidTokenLifetime,
    #[error("jwt lifetime exceeds configured maximum")]
    TokenLifetimeExceeded,
}

#[async_trait]
pub(super) trait JwksSource: Send + Sync {
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
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("JWKS HTTP client configuration is valid"),
            jwks_url,
        }
    }
}

#[async_trait]
impl JwksSource for RemoteJwksSource {
    async fn load_jwks(&self) -> anyhow::Result<JwkSet> {
        let response = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .with_context(|| format!("failed to fetch JWKS from {}", self.jwks_url))?
            .error_for_status()
            .with_context(|| format!("JWKS endpoint returned error for {}", self.jwks_url))?;
        const MAX_JWKS_BYTES: usize = 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_JWKS_BYTES as u64)
        {
            anyhow::bail!("JWKS response exceeds {MAX_JWKS_BYTES} bytes");
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed to read JWKS response body")?;
            if body.len().saturating_add(chunk.len()) > MAX_JWKS_BYTES {
                anyhow::bail!("JWKS response exceeds {MAX_JWKS_BYTES} bytes");
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice::<JwkSet>(&body)
            .with_context(|| format!("failed to decode JWKS from {}", self.jwks_url))
    }
}

const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);
const JWKS_REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CachedJwks {
    keys: JwkSet,
    fetched_at: Instant,
}

#[derive(Clone)]
pub struct JwtVerifier {
    config: ExternalAuthConfig,
    jwks_source: Arc<dyn JwksSource>,
    cache: Arc<RwLock<Option<CachedJwks>>>,
    refresh_lock: Arc<Mutex<()>>,
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
            cache: Arc::new(RwLock::new(Some(CachedJwks {
                keys: jwks,
                fetched_at: Instant::now(),
            }))),
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(super) fn new(config: ExternalAuthConfig, jwks_source: Arc<dyn JwksSource>) -> Self {
        Self {
            config,
            jwks_source,
            cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
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
                let jwks = self.refresh_for_unknown_key().await?;
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

        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "sub", "aud"]);

        let claims = decode::<JwtClaims>(token, &key, &validation)
            .map_err(JwtVerifyError::InvalidClaims)?
            .claims;
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64;
        if claims.exp < claims.iat || claims.iat > now.saturating_add(self.config.leeway_seconds) {
            return Err(JwtVerifyError::InvalidTokenLifetime);
        }
        if claims.exp - claims.iat > self.config.max_token_lifetime_seconds {
            return Err(JwtVerifyError::TokenLifetimeExceeded);
        }
        verified_identity(&self.config, claims)
    }

    pub async fn check_ready(&self) -> Result<(), JwtVerifyError> {
        self.cached_or_fetch().await.map(|_| ())
    }

    #[cfg(test)]
    pub(super) async fn expire_cache_for_test(&self) {
        if let Some(cached) = self.cache.write().await.as_mut() {
            cached.fetched_at = Instant::now() - JWKS_CACHE_TTL;
        }
    }

    async fn cached_or_fetch(&self) -> Result<JwkSet, JwtVerifyError> {
        if let Some(cached) = self.cache.read().await.clone()
            && cached.fetched_at.elapsed() < JWKS_CACHE_TTL
        {
            return Ok(cached.keys);
        }

        self.refresh(false).await
    }

    async fn refresh_for_unknown_key(&self) -> Result<JwkSet, JwtVerifyError> {
        self.refresh(true).await
    }

    async fn refresh(&self, unknown_key: bool) -> Result<JwkSet, JwtVerifyError> {
        let _guard = self.refresh_lock.lock().await;
        if let Some(cached) = self.cache.read().await.clone() {
            let max_age = if unknown_key {
                JWKS_REFRESH_COOLDOWN
            } else {
                JWKS_CACHE_TTL
            };
            if cached.fetched_at.elapsed() < max_age {
                return Ok(cached.keys);
            }
        }
        let jwks = self
            .jwks_source
            .load_jwks()
            .await
            .map_err(JwtVerifyError::Jwks)?;
        *self.cache.write().await = Some(CachedJwks {
            keys: jwks.clone(),
            fetched_at: Instant::now(),
        });
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
