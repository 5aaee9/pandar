pub(crate) fn tenant_self_create_allowed_from_env() -> anyhow::Result<bool> {
    env_bool("PANDAR_AUTH_ALLOW_TENANT_SELF_CREATE", true)
}

pub(crate) fn no_auth_from_env() -> anyhow::Result<bool> {
    env_bool("PANDAR_HUB_NO_AUTH", false)
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
