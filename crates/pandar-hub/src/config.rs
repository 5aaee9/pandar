use anyhow::Context;

use crate::camera_sessions::DEFAULT_MAX_CAMERA_STREAMS_PER_TENANT;

pub(crate) fn tenant_self_create_allowed_from_env() -> anyhow::Result<bool> {
    env_bool("PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE", true)
}

pub(crate) fn no_auth_from_env() -> anyhow::Result<bool> {
    env_bool("PANDAR_HUB_NO_AUTH", false)
}

pub(crate) fn camera_max_streams_per_tenant_from_env() -> anyhow::Result<usize> {
    let name = "PANDAR_HUB_CAMERA_MAX_STREAMS_PER_TENANT";
    let value = std::env::var(name).ok();
    parse_camera_max_streams_per_tenant(name, value.as_deref())
}

fn parse_camera_max_streams_per_tenant(name: &str, value: Option<&str>) -> anyhow::Result<usize> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(DEFAULT_MAX_CAMERA_STREAMS_PER_TENANT);
    };
    let value = value
        .trim()
        .parse::<usize>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    if value == 0 {
        anyhow::bail!("{name} must be a positive integer");
    }
    Ok(value)
}

fn env_bool(name: &str, default: bool) -> anyhow::Result<bool> {
    let Some(value) = std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(default);
    };
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => anyhow::bail!("{name} must be true or false"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_stream_limit_defaults_to_eight() {
        assert_eq!(
            parse_camera_max_streams_per_tenant("camera limit", None).unwrap(),
            8
        );
        assert_eq!(
            parse_camera_max_streams_per_tenant("camera limit", Some(" ")).unwrap(),
            8
        );
    }

    #[test]
    fn camera_stream_limit_accepts_positive_values() {
        assert_eq!(
            parse_camera_max_streams_per_tenant("camera limit", Some("12")).unwrap(),
            12
        );
    }

    #[test]
    fn camera_stream_limit_rejects_zero_and_invalid_values() {
        assert!(parse_camera_max_streams_per_tenant("camera limit", Some("0")).is_err());
        assert!(parse_camera_max_streams_per_tenant("camera limit", Some("many")).is_err());
    }
}
