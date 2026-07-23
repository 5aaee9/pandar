use std::{fs, path::Path};

use anyhow::{Context, ensure};

use super::{
    LOGIN_FILE_LOCK, PENDING_QUEUE_LOCK, completed, durable, load_login_unlocked,
    load_pending_unlocked, matches_login, store_pending,
};
use crate::account::types::PendingRevocation;

const DIRECT_REVOCATION_FILE: &str = "pandar-plugin-direct-revocation.json";

pub(super) fn prepare_login(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<durable::MutationDurability> {
    let _process_guard = super::process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    ensure!(
        !config_dir.is_empty(),
        "direct revocation has no Studio config directory"
    );
    let login = load_login_unlocked(config_dir)?
        .context("direct revocation has no persisted Studio login")?;
    ensure!(
        matches_login(&login, candidate),
        "direct revocation does not match the persisted Studio login"
    );
    prepare_unlocked(config_dir, candidate)
}

pub(super) fn prepare_orphan(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<durable::MutationDurability> {
    let _process_guard = super::process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    ensure!(
        !config_dir.is_empty(),
        "direct revocation has no Studio config directory"
    );
    prepare_unlocked(config_dir, candidate)
}

fn prepare_unlocked(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<durable::MutationDurability> {
    let directory = Path::new(config_dir);
    fs::create_dir_all(directory).context("create direct revocation directory")?;
    if let Some(existing) = load_unlocked(config_dir)? {
        ensure!(
            existing == *candidate,
            "another direct Studio revocation is already pending"
        );
        return Ok(match durable::confirm(directory) {
            Ok(()) => durable::MutationDurability::Confirmed,
            Err(error) => durable::MutationDurability::ChangedUnconfirmed(
                error.context("confirm existing direct Studio revocation"),
            ),
        });
    }

    let path = directory.join(DIRECT_REVOCATION_FILE);
    let body = serde_json::to_vec(candidate).context("encode direct Studio revocation")?;
    durable::write_replace(directory, &path, &body)
}

#[cfg(test)]
pub(super) fn load(config_dir: &str) -> anyhow::Result<Option<PendingRevocation>> {
    let _process_guard = super::process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    let revocation = load_unlocked(config_dir)?;
    if revocation.is_some() {
        durable::confirm(Path::new(config_dir))
            .context("confirm direct Studio revocation before replay")?;
    }
    Ok(revocation)
}

pub(super) fn load_unlocked(config_dir: &str) -> anyhow::Result<Option<PendingRevocation>> {
    if config_dir.is_empty() {
        return Ok(None);
    }
    let path = Path::new(config_dir).join(DIRECT_REVOCATION_FILE);
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read direct Studio revocation"),
    };
    serde_json::from_str(&body).context("decode direct Studio revocation")
}

pub(super) fn complete(config_dir: &str, candidate: &PendingRevocation) -> anyhow::Result<()> {
    let _process_guard = super::process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    let _pending_guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    completed::record_unlocked(config_dir, candidate)?;
    if load_login_unlocked(config_dir)?
        .as_ref()
        .is_some_and(|login| matches_login(login, candidate))
    {
        super::clear_login_unlocked(config_dir)?
            .require_confirmed("durably clear direct-revocation Studio login")?;
    }
    let directory = Path::new(config_dir);
    if let Some(existing) = load_unlocked(config_dir)? {
        ensure!(
            existing == *candidate,
            "direct Studio revocation changed before completion"
        );
        durable::remove(
            &directory.join(DIRECT_REVOCATION_FILE),
            directory,
            "complete direct Studio revocation",
        )?
        .report("pandar direct Studio revocation completion durability warning");
    }
    match load_pending_unlocked(config_dir) {
        Ok(mut pending) => {
            let previous_len = pending.len();
            pending.retain(|revocation| revocation != candidate);
            if pending.len() != previous_len {
                match store_pending(config_dir, &pending) {
                    Ok(durability) => durability
                        .report("pandar duplicate pending revocation cleanup durability warning"),
                    Err(error) => {
                        eprintln!("pandar duplicate pending revocation cleanup failed: {error:#}")
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("pandar duplicate pending revocation lookup failed: {error:#}");
        }
    }
    Ok(())
}
