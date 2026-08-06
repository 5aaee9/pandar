use std::{fs, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::durable;
use crate::account::types::{PendingRevocation, PersistedLogin};

const COMPLETED_REVOCATIONS_FILE: &str = "pandar-plugin-completed-revocations.json";

#[derive(Deserialize, Eq, PartialEq, Serialize)]
struct CompletedRevocation {
    hub_url: String,
    token_sha256: String,
}

pub(super) fn contains_login_unlocked(
    config_dir: &str,
    login: &PersistedLogin,
) -> anyhow::Result<bool> {
    contains_unlocked(
        config_dir,
        &CompletedRevocation {
            hub_url: login.hub_url.clone(),
            token_sha256: token_sha256(&login.token),
        },
    )
}

pub(super) fn record_unlocked(
    config_dir: &str,
    revocation: &PendingRevocation,
) -> anyhow::Result<()> {
    let directory = Path::new(config_dir);
    let mut completed = load_unlocked(config_dir)?;
    let entry = CompletedRevocation::from(revocation);
    if completed.contains(&entry) {
        return durable::confirm(directory)
            .context("confirm completed Studio revocation tombstone");
    }
    completed.push(entry);
    let body = serde_json::to_vec(&completed).context("encode completed Studio revocations")?;
    durable::write_replace(
        directory,
        &directory.join(COMPLETED_REVOCATIONS_FILE),
        &body,
    )?
    .require_confirmed("durably record completed Studio revocation")
}

fn contains_unlocked(config_dir: &str, expected: &CompletedRevocation) -> anyhow::Result<bool> {
    Ok(load_unlocked(config_dir)?.contains(expected))
}

fn load_unlocked(config_dir: &str) -> anyhow::Result<Vec<CompletedRevocation>> {
    if config_dir.is_empty() {
        return Ok(Vec::new());
    }
    let path = Path::new(config_dir).join(COMPLETED_REVOCATIONS_FILE);
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("read completed Studio revocations"),
    };
    serde_json::from_str(&body).context("decode completed Studio revocations")
}

impl From<&PendingRevocation> for CompletedRevocation {
    fn from(revocation: &PendingRevocation) -> Self {
        Self {
            hub_url: revocation.hub_url.clone(),
            token_sha256: token_sha256(&revocation.token),
        }
    }
}

fn token_sha256(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
