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

#[derive(Debug, PartialEq, Eq)]
pub enum PreflightError {
    Missing(Vec<&'static str>),
    UnsafeDatabaseUrl,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(names) => write!(
                formatter,
                "missing live soak environment variables: {}",
                names.join(", ")
            ),
            Self::UnsafeDatabaseUrl => write!(
                formatter,
                "PANDAR_SOAK_DATABASE_URL must point to disposable soak data, not production"
            ),
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

    let database_url = values
        .get("PANDAR_SOAK_DATABASE_URL")
        .expect("missing database URL was checked above")
        .to_ascii_lowercase();
    if database_url.contains("production") || database_url.contains("prod") {
        return Err(PreflightError::UnsafeDatabaseUrl);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_values() -> BTreeMap<String, String> {
        REQUIRED_ENV
            .iter()
            .map(|name| ((*name).to_owned(), format!("value-for-{name}")))
            .collect()
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

        assert_eq!(validate(&values), Err(PreflightError::UnsafeDatabaseUrl));
    }
}
