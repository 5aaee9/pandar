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
                write!(formatter, "invalid live soak environment variables: {details}")
            }
        }
    }
}

pub fn run_preflight() -> anyhow::Result<()> {
    let values = REQUIRED_ENV
        .iter()
        .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
        .collect::<BTreeMap<_, _>>();
    validate(&values).map_err(|error| anyhow::anyhow!("{error}"))?;
    println!("PASS live soak preflight");
    Ok(())
}

pub fn validate(values: &BTreeMap<String, String>) -> Result<(), PreflightError> {
    let missing = REQUIRED_ENV
        .iter()
        .copied()
        .filter(|name| values.get(*name).is_none_or(|value| value.trim().is_empty()))
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
