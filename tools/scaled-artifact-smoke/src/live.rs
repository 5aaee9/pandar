use std::{collections::BTreeMap, env, fmt};

pub const REQUIRED_ENV: &[&str] = &[
    "PANDAR_SOAK_DATABASE_URL",
    "PANDAR_SOAK_NATS_URL",
    "PANDAR_SOAK_ARTIFACT_S3_BUCKET",
    "PANDAR_SOAK_ARTIFACT_S3_REGION",
    "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT",
    "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID",
    "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
];

pub const DEFAULT_SOAK_NATS_SUBJECT: &str = "pandar.soak.control";
const OPTIONAL_ENV: &[&str] = &[
    "PANDAR_SOAK_NATS_SUBJECT",
    "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE",
];

#[derive(Debug, Clone)]
pub struct LiveConfig {
    pub database_url: String,
    pub nats_url: String,
    pub nats_subject: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: String,
    pub s3_access_key_id: String,
    pub s3_secret_access_key: String,
    pub s3_force_path_style: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidVariable {
    pub name: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreflightError {
    Missing(Vec<&'static str>),
    Invalid(Vec<InvalidVariable>),
}

impl fmt::Display for PreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(names) => write!(
                formatter,
                "missing live soak environment variables: {}",
                names.join(", ")
            ),
            Self::Invalid(values) => {
                let details = values
                    .iter()
                    .map(|value| format!("{} ({})", value.name, value.reason))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "invalid live soak environment variables: {details}"
                )
            }
        }
    }
}

pub fn run_preflight() -> anyhow::Result<()> {
    LiveConfig::from_values(collect_env())?;
    println!("PASS live soak preflight");
    Ok(())
}

pub fn collect_env() -> BTreeMap<String, String> {
    REQUIRED_ENV
        .iter()
        .chain(OPTIONAL_ENV.iter())
        .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect()
}

impl LiveConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_values(collect_env())
    }

    pub fn from_values(values: BTreeMap<String, String>) -> anyhow::Result<Self> {
        validate(&values).map_err(|error| anyhow::anyhow!("{error}"))?;
        let s3_force_path_style =
            parse_optional_bool(&values, "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE", true)?;

        Ok(Self {
            database_url: required_value(&values, "PANDAR_SOAK_DATABASE_URL").to_owned(),
            nats_url: required_value(&values, "PANDAR_SOAK_NATS_URL").to_owned(),
            nats_subject: optional_value(&values, "PANDAR_SOAK_NATS_SUBJECT")
                .unwrap_or(DEFAULT_SOAK_NATS_SUBJECT)
                .to_owned(),
            s3_bucket: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_BUCKET").to_owned(),
            s3_region: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_REGION").to_owned(),
            s3_endpoint: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT").to_owned(),
            s3_access_key_id: required_value(&values, "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID")
                .to_owned(),
            s3_secret_access_key: required_value(
                &values,
                "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
            )
            .to_owned(),
            s3_force_path_style,
        })
    }
}

pub fn validate(values: &BTreeMap<String, String>) -> Result<(), PreflightError> {
    let missing = REQUIRED_ENV
        .iter()
        .copied()
        .filter(|name| {
            values
                .get(*name)
                .is_none_or(|value| value.trim().is_empty())
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PreflightError::Missing(missing));
    }

    let mut invalid = Vec::new();

    let database_url = required_value(values, "PANDAR_SOAK_DATABASE_URL").to_ascii_lowercase();
    if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
        invalid.push(InvalidVariable {
            name: "PANDAR_SOAK_DATABASE_URL",
            reason: "must start with postgres:// or postgresql://",
        });
    } else if !contains_any(&database_url, &["soak", "disposable", "ephemeral", "test"]) {
        invalid.push(InvalidVariable {
            name: "PANDAR_SOAK_DATABASE_URL",
            reason: "must contain a disposable marker",
        });
    } else if contains_any(&database_url, &["prod", "production"]) {
        invalid.push(InvalidVariable {
            name: "PANDAR_SOAK_DATABASE_URL",
            reason: "must not contain production markers",
        });
    }

    if !required_value(values, "PANDAR_SOAK_NATS_URL").starts_with("nats://") {
        invalid.push(InvalidVariable {
            name: "PANDAR_SOAK_NATS_URL",
            reason: "must start with nats://",
        });
    }

    let endpoint = required_value(values, "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT");
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        invalid.push(InvalidVariable {
            name: "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT",
            reason: "must start with http:// or https://",
        });
    }

    for name in [
        "PANDAR_SOAK_ARTIFACT_S3_BUCKET",
        "PANDAR_SOAK_ARTIFACT_S3_REGION",
        "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID",
        "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
    ] {
        if is_placeholder(required_value(values, name)) {
            invalid.push(InvalidVariable {
                name,
                reason: "must not be blank or placeholder-looking",
            });
        }
    }

    if !invalid.is_empty() {
        return Err(PreflightError::Invalid(invalid));
    }

    Ok(())
}

fn required_value<'a>(values: &'a BTreeMap<String, String>, name: &str) -> &'a str {
    values
        .get(name)
        .expect("missing values were checked above")
        .trim()
}

fn optional_value<'a>(values: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    values
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn parse_optional_bool(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: bool,
) -> anyhow::Result<bool> {
    match optional_value(values, name) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => anyhow::bail!("{name} must be true or false"),
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn is_placeholder(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.is_empty()
        || (value.starts_with('<') && value.ends_with('>'))
        || value.starts_with("value-for-")
        || matches!(
            value.as_str(),
            "bucket" | "region" | "access-key" | "secret" | "changeme"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_values() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "PANDAR_SOAK_DATABASE_URL".to_owned(),
                "postgres://pandar_soak@localhost/pandar_soak".to_owned(),
            ),
            (
                "PANDAR_SOAK_NATS_URL".to_owned(),
                "nats://127.0.0.1:4222".to_owned(),
            ),
            (
                "PANDAR_SOAK_ARTIFACT_S3_BUCKET".to_owned(),
                "pandar-soak-artifacts".to_owned(),
            ),
            (
                "PANDAR_SOAK_ARTIFACT_S3_REGION".to_owned(),
                "us-east-1".to_owned(),
            ),
            (
                "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT".to_owned(),
                "http://127.0.0.1:9000".to_owned(),
            ),
            (
                "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID".to_owned(),
                "pandar-soak-access".to_owned(),
            ),
            (
                "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY".to_owned(),
                "pandar-soak-secret".to_owned(),
            ),
        ])
    }

    fn invalid_names(error: PreflightError) -> Vec<&'static str> {
        match error {
            PreflightError::Invalid(values) => values.into_iter().map(|value| value.name).collect(),
            other => panic!("expected invalid variables, got {other:?}"),
        }
    }

    #[test]
    fn live_config_defaults_optional_values() {
        let config = LiveConfig::from_values(complete_values()).unwrap();
        assert_eq!(
            config.database_url,
            "postgres://pandar_soak@localhost/pandar_soak"
        );
        assert_eq!(config.nats_url, "nats://127.0.0.1:4222");
        assert_eq!(config.nats_subject, "pandar.soak.control");
        assert!(config.s3_force_path_style);
    }

    #[test]
    fn live_config_accepts_optional_values() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_NATS_SUBJECT".to_owned(),
            "pandar.custom.soak".to_owned(),
        );
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE".to_owned(),
            "false".to_owned(),
        );

        let config = LiveConfig::from_values(values).unwrap();
        assert_eq!(config.nats_subject, "pandar.custom.soak");
        assert!(!config.s3_force_path_style);
    }

    #[test]
    fn live_config_rejects_invalid_path_style_with_soak_name() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE".to_owned(),
            "maybe".to_owned(),
        );

        let err = LiveConfig::from_values(values).unwrap_err();
        assert!(format!("{err:#}").contains("PANDAR_SOAK_ARTIFACT_S3_FORCE_PATH_STYLE"));
    }

    #[test]
    fn live_config_ignores_production_s3_environment_names() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_ARTIFACT_S3_BUCKET".to_owned(),
            "production-bucket".to_owned(),
        );
        values.insert(
            "PANDAR_ARTIFACT_S3_REGION".to_owned(),
            "production-region".to_owned(),
        );
        values.insert(
            "PANDAR_ARTIFACT_S3_ENDPOINT".to_owned(),
            "https://production.example.invalid".to_owned(),
        );
        values.insert(
            "PANDAR_ARTIFACT_S3_ACCESS_KEY_ID".to_owned(),
            "production-access".to_owned(),
        );
        values.insert(
            "PANDAR_ARTIFACT_S3_SECRET_ACCESS_KEY".to_owned(),
            "production-secret".to_owned(),
        );

        let config = LiveConfig::from_values(values).unwrap();
        assert_eq!(config.s3_bucket, "pandar-soak-artifacts");
        assert_eq!(config.s3_region, "us-east-1");
        assert_eq!(config.s3_endpoint, "http://127.0.0.1:9000");
        assert_eq!(config.s3_access_key_id, "pandar-soak-access");
        assert_eq!(config.s3_secret_access_key, "pandar-soak-secret");
    }

    #[test]
    fn validate_reports_all_missing_variables() {
        assert_eq!(
            validate(&BTreeMap::new()),
            Err(PreflightError::Missing(REQUIRED_ENV.to_vec()))
        );
    }

    #[test]
    fn validate_accepts_complete_disposable_inputs() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar_soak@localhost/pandar_soak".to_owned(),
        );

        assert_eq!(validate(&values), Ok(()));
    }

    #[test]
    fn validate_rejects_production_database_url() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar@db.example.com/pandar-PRODuction".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_DATABASE_URL"]
        );
    }

    #[test]
    fn validate_rejects_database_url_without_disposable_marker() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar@localhost/pandar".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_DATABASE_URL"]
        );
    }

    #[test]
    fn validate_rejects_database_url_with_disposable_and_production_markers() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "postgres://pandar_soak@localhost/pandar_soak_prod".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_DATABASE_URL"]
        );
    }

    #[test]
    fn validate_rejects_non_postgres_database_url() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_DATABASE_URL".to_owned(),
            "sqlite://pandar_soak.db".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_DATABASE_URL"]
        );
    }

    #[test]
    fn validate_rejects_non_nats_url() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_NATS_URL".to_owned(),
            "http://127.0.0.1:4222".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_NATS_URL"]
        );
    }

    #[test]
    fn validate_rejects_non_http_s3_endpoint() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_ENDPOINT".to_owned(),
            "s3://pandar-soak".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec!["PANDAR_SOAK_ARTIFACT_S3_ENDPOINT"]
        );
    }

    #[test]
    fn validate_reports_placeholder_storage_values_together() {
        let mut values = complete_values();
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_BUCKET".to_owned(),
            "<bucket>".to_owned(),
        );
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_REGION".to_owned(),
            "region".to_owned(),
        );
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID".to_owned(),
            "value-for-PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID".to_owned(),
        );
        values.insert(
            "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY".to_owned(),
            "changeme".to_owned(),
        );

        assert_eq!(
            invalid_names(validate(&values).unwrap_err()),
            vec![
                "PANDAR_SOAK_ARTIFACT_S3_BUCKET",
                "PANDAR_SOAK_ARTIFACT_S3_REGION",
                "PANDAR_SOAK_ARTIFACT_S3_ACCESS_KEY_ID",
                "PANDAR_SOAK_ARTIFACT_S3_SECRET_ACCESS_KEY",
            ]
        );
    }
}
