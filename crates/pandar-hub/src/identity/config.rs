use std::{collections::HashMap, env};

use jsonwebtoken::Algorithm;
use thiserror::Error;

const PROVIDER_VAR: &str = "PANDAR_EXTERNAL_AUTH_PROVIDER";
const ISSUER_VAR: &str = "PANDAR_EXTERNAL_AUTH_ISSUER";
const JWKS_URL_VAR: &str = "PANDAR_EXTERNAL_AUTH_JWKS_URL";
const AUDIENCE_VAR: &str = "PANDAR_EXTERNAL_AUTH_AUDIENCE";
const ALGORITHMS_VAR: &str = "PANDAR_EXTERNAL_AUTH_ALGORITHMS";
const AUTHORIZED_PARTIES_VAR: &str = "PANDAR_EXTERNAL_AUTH_AUTHORIZED_PARTIES";
const REQUIRED_SCOPES_VAR: &str = "PANDAR_EXTERNAL_AUTH_REQUIRED_SCOPES";
const LEEWAY_SECONDS_VAR: &str = "PANDAR_EXTERNAL_AUTH_LEEWAY_SECONDS";

const EXTERNAL_AUTH_VARS: [&str; 8] = [
    PROVIDER_VAR,
    ISSUER_VAR,
    JWKS_URL_VAR,
    AUDIENCE_VAR,
    ALGORITHMS_VAR,
    AUTHORIZED_PARTIES_VAR,
    REQUIRED_SCOPES_VAR,
    LEEWAY_SECONDS_VAR,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAuthConfig {
    pub provider: String,
    pub issuer: String,
    pub jwks_url: String,
    pub audience: Option<String>,
    pub algorithms: Vec<Algorithm>,
    pub authorized_parties: Vec<String>,
    pub required_scopes: Vec<String>,
    pub leeway_seconds: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExternalAuthConfigError {
    #[error("partial external auth config without provider")]
    PartialWithoutProvider,
    #[error("missing external auth config value: {0}")]
    Missing(&'static str),
    #[error("unsupported external auth algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("invalid external auth leeway seconds")]
    InvalidLeeway,
}

impl ExternalAuthConfig {
    pub fn from_env() -> Result<Option<Self>, ExternalAuthConfigError> {
        Self::from_vars(EXTERNAL_AUTH_VARS.into_iter().filter_map(|key| {
            env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (key.to_owned(), value))
        }))
    }

    pub(super) fn from_vars(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Option<Self>, ExternalAuthConfigError> {
        let vars = vars.into_iter().collect::<HashMap<_, _>>();
        let Some(provider) = value(&vars, PROVIDER_VAR) else {
            if EXTERNAL_AUTH_VARS[1..]
                .iter()
                .any(|key| value(&vars, key).is_some())
            {
                return Err(ExternalAuthConfigError::PartialWithoutProvider);
            }
            return Ok(None);
        };

        let issuer = required(&vars, ISSUER_VAR)?;
        let jwks_url = required(&vars, JWKS_URL_VAR)?;
        let audience = value(&vars, AUDIENCE_VAR);
        let algorithms = value(&vars, ALGORITHMS_VAR)
            .map(|value| parse_algorithms(&value))
            .transpose()?
            .unwrap_or_else(|| vec![Algorithm::RS256]);
        let authorized_parties = parse_csv(value(&vars, AUTHORIZED_PARTIES_VAR));
        let required_scopes = parse_csv(value(&vars, REQUIRED_SCOPES_VAR));
        let leeway_seconds = value(&vars, LEEWAY_SECONDS_VAR)
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| ExternalAuthConfigError::InvalidLeeway)
            })
            .transpose()?
            .unwrap_or(60);

        Ok(Some(Self {
            provider,
            issuer,
            jwks_url,
            audience,
            algorithms,
            authorized_parties,
            required_scopes,
            leeway_seconds,
        }))
    }
}

fn value(vars: &HashMap<String, String>, key: &str) -> Option<String> {
    vars.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required(
    vars: &HashMap<String, String>,
    key: &'static str,
) -> Result<String, ExternalAuthConfigError> {
    value(vars, key).ok_or(ExternalAuthConfigError::Missing(key))
}

fn parse_csv(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn parse_algorithms(value: &str) -> Result<Vec<Algorithm>, ExternalAuthConfigError> {
    let algorithms = parse_csv(Some(value.to_owned()));
    if algorithms.is_empty() {
        return Ok(vec![Algorithm::RS256]);
    }

    algorithms
        .into_iter()
        .map(|algorithm| match algorithm.as_str() {
            "RS256" => Ok(Algorithm::RS256),
            "RS384" => Ok(Algorithm::RS384),
            "RS512" => Ok(Algorithm::RS512),
            _ => Err(ExternalAuthConfigError::UnsupportedAlgorithm(algorithm)),
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) fn config_from_vars(
        extra: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> ExternalAuthConfig {
        ExternalAuthConfig::from_vars(base_vars(extra))
            .unwrap()
            .unwrap()
    }

    pub(crate) fn base_vars(
        extra: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Vec<(String, String)> {
        let mut vars = vec![
            (PROVIDER_VAR.to_owned(), "clerk".to_owned()),
            (
                ISSUER_VAR.to_owned(),
                "https://issuer.example.test".to_owned(),
            ),
            (
                JWKS_URL_VAR.to_owned(),
                "https://issuer.example.test/.well-known/jwks.json".to_owned(),
            ),
        ];
        vars.extend(
            extra
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned())),
        );
        vars
    }

    pub(crate) const AUDIENCE_VAR: &str = super::AUDIENCE_VAR;
    pub(crate) const ALGORITHMS_VAR: &str = super::ALGORITHMS_VAR;
    pub(crate) const ISSUER_VAR: &str = super::ISSUER_VAR;
    pub(crate) const REQUIRED_SCOPES_VAR: &str = super::REQUIRED_SCOPES_VAR;
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalAuthConfig, ExternalAuthConfigError,
        tests_support::{ALGORITHMS_VAR, ISSUER_VAR, base_vars, config_from_vars},
    };
    use jsonwebtoken::Algorithm;

    #[test]
    fn partial_env_config_returns_error() {
        let result = ExternalAuthConfig::from_vars([(
            ISSUER_VAR.to_owned(),
            "https://issuer.example.test".to_owned(),
        )]);

        assert_eq!(result, Err(ExternalAuthConfigError::PartialWithoutProvider));
    }

    #[test]
    fn default_algorithm_is_rs256() {
        let config = config_from_vars([]);

        assert_eq!(config.algorithms, vec![Algorithm::RS256]);
    }

    #[test]
    fn unsupported_algorithm_config_rejected() {
        let result = ExternalAuthConfig::from_vars(base_vars([(ALGORITHMS_VAR, "HS256")]));

        assert_eq!(
            result,
            Err(ExternalAuthConfigError::UnsupportedAlgorithm(
                "HS256".to_owned()
            ))
        );
    }
}
