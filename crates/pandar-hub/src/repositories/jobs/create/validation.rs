use anyhow::Context;

use crate::{
    material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo, validate_mapping_len},
    repositories::{RepositoryError, RepositoryResult},
};

pub(super) fn validate_mapping_json(
    value: &Option<String>,
    field: &'static str,
) -> RepositoryResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = match field {
        "ams_mapping_json" => serde_json::from_str::<AmsMapping>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        "ams_mapping2_json" => serde_json::from_str::<AmsMapping2>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        "ams_mapping_info_json" => serde_json::from_str::<AmsMappingInfo>(value)
            .with_context(|| format!("failed to validate {field}"))?
            .len(),
        _ => unreachable!("validated mapping field should be known"),
    };
    if !validate_mapping_len(len) {
        return Err(RepositoryError::Database(anyhow::anyhow!(
            "{field} must not contain more than 32 entries"
        )));
    }
    Ok(())
}
