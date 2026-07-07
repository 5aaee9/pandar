use anyhow::Context;

use crate::repositories::{RepositoryError, RepositoryResult};

pub(super) fn validate_mapping_json(
    value: &Option<String>,
    field: &'static str,
) -> RepositoryResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = match field {
        "ams_mapping_json" => serde_json::from_str::<Vec<i32>>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        "ams_mapping2_json" => {
            let entries = serde_json::from_str::<
                Vec<crate::repositories::jobs::print_reports::usage::Mapping2Entry>,
            >(value)
            .with_context(|| format!("failed to validate {field}"))?;
            entries.len()
        }
        "ams_mapping_info_json" => serde_json::from_str::<Vec<serde_json::Value>>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        _ => unreachable!("validated mapping field should be known"),
    };
    if len > 32 {
        return Err(RepositoryError::Database(anyhow::anyhow!(
            "{field} must not contain more than 32 entries"
        )));
    }
    Ok(())
}
