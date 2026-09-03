use std::{fs, path::Path, sync::Mutex};

use anyhow::{Context, ensure};

use super::types::{PendingRevocation, PersistedLogin};

mod completed;
mod direct;
mod durable;
mod process_lock;

#[cfg(test)]
pub(super) use durable::{FaultPoint, fail_next};
pub(in crate::account) use durable::{MutationDurability, write_replace as durable_write_replace};
pub(in crate::account) use process_lock::acquire as acquire_process_lock;

pub(super) enum PersistedRevocation {
    Direct(PendingRevocation),
    Pending(PendingRevocation),
}

static LOGIN_FILE_LOCK: Mutex<()> = Mutex::new(());
static PENDING_QUEUE_LOCK: Mutex<()> = Mutex::new(());
const LOGIN_FILE: &str = "pandar-plugin-login.json";
const PENDING_REVOCATIONS_FILE: &str = "pandar-plugin-pending-revocations.json";

pub(super) fn load(config_dir: &str) -> anyhow::Result<Option<PersistedLogin>> {
    load_snapshot(config_dir, || {})
}

fn load_snapshot(
    config_dir: &str,
    after_login_lock: impl FnOnce(),
) -> anyhow::Result<Option<PersistedLogin>> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _login_guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    after_login_lock();
    let _pending_guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    let login = load_login_unlocked(config_dir)?;
    let Some(login) = login else {
        return Ok(None);
    };
    if direct::load_unlocked(config_dir)?
        .as_ref()
        .is_some_and(|candidate| matches_login(&login, candidate))
    {
        return Ok(None);
    }
    if completed::contains_login_unlocked(config_dir, &login)? {
        return Ok(None);
    }
    let pending = load_pending_unlocked(config_dir)?;
    if pending
        .iter()
        .any(|candidate| matches_login(&login, candidate))
    {
        return Ok(None);
    }
    Ok(Some(login))
}

fn load_login_unlocked(config_dir: &str) -> anyhow::Result<Option<PersistedLogin>> {
    if config_dir.is_empty() {
        return Ok(None);
    }
    let path = Path::new(config_dir).join(LOGIN_FILE);
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read persisted Studio login"),
    };
    serde_json::from_str(&body).context("decode persisted Studio login")
}

pub(super) fn store(
    config_dir: &str,
    login: &PersistedLogin,
) -> anyhow::Result<MutationDurability> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    if config_dir.is_empty() {
        return Ok(MutationDurability::Confirmed);
    }
    let directory = Path::new(config_dir);
    fs::create_dir_all(directory).context("create Studio login directory")?;
    let path = directory.join(LOGIN_FILE);
    let _pending_guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    let blocked = direct::load_unlocked(config_dir)?
        .as_ref()
        .is_some_and(|candidate| matches_login(login, candidate))
        || load_pending_unlocked(config_dir)?
            .iter()
            .any(|candidate| matches_login(login, candidate))
        || completed::contains_login_unlocked(config_dir, login)?;
    ensure!(!blocked, "refuse to persist a revoked Studio login");
    let body = serde_json::to_vec(login).context("encode persisted Studio login")?;
    Ok(durable::write_replace(directory, &path, &body)?
        .reconfirm(directory, "confirm persisted Studio login replacement"))
}

pub(super) fn clear(config_dir: &str) -> anyhow::Result<MutationDurability> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    clear_login_unlocked(config_dir)
}

pub(super) fn clear_matching(
    config_dir: &str,
    revocation: &PendingRevocation,
) -> anyhow::Result<MutationDurability> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    if load_login_unlocked(config_dir)?
        .as_ref()
        .is_some_and(|login| matches_login(login, revocation))
    {
        return clear_login_unlocked(config_dir);
    }
    Ok(MutationDurability::Confirmed)
}

fn clear_login_unlocked(config_dir: &str) -> anyhow::Result<MutationDurability> {
    if config_dir.is_empty() {
        return Ok(MutationDurability::Confirmed);
    }
    let path = Path::new(config_dir).join(LOGIN_FILE);
    let directory = Path::new(config_dir);
    Ok(
        durable::remove(&path, directory, "remove persisted Studio login")?
            .reconfirm(directory, "confirm persisted Studio login removal"),
    )
}

fn matches_login(login: &PersistedLogin, revocation: &PendingRevocation) -> bool {
    login.hub_url == revocation.hub_url && login.token == revocation.token
}

#[cfg(test)]
pub(super) fn load_pending(config_dir: &str) -> anyhow::Result<Vec<PendingRevocation>> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    load_pending_unlocked(config_dir)
}

fn load_pending_unlocked(config_dir: &str) -> anyhow::Result<Vec<PendingRevocation>> {
    if config_dir.is_empty() {
        return Ok(Vec::new());
    }
    let path = Path::new(config_dir).join(PENDING_REVOCATIONS_FILE);
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("read pending plugin revocations"),
    };
    serde_json::from_str(&body).context("decode pending plugin revocations")
}

pub(super) fn enqueue_pending(
    config_dir: &str,
    revocation: PendingRevocation,
) -> anyhow::Result<MutationDurability> {
    ensure!(
        !config_dir.is_empty(),
        "pending revocation has no Studio config directory"
    );
    mutate_pending(config_dir, |pending| {
        if pending.contains(&revocation) {
            false
        } else {
            pending.push(revocation);
            true
        }
    })
}

#[cfg(test)]
pub(super) fn remove_pending(
    config_dir: &str,
    revocation: &PendingRevocation,
) -> anyhow::Result<MutationDurability> {
    mutate_pending(config_dir, |pending| {
        let previous_len = pending.len();
        pending.retain(|candidate| candidate != revocation);
        pending.len() != previous_len
    })
}

pub(super) fn load_next_revocation(
    config_dir: &str,
) -> anyhow::Result<Option<PersistedRevocation>> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _login_guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    let _pending_guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    if let Some(revocation) = direct::load_unlocked(config_dir)? {
        durable::confirm(Path::new(config_dir))
            .context("confirm direct Studio revocation before replay")?;
        return Ok(Some(PersistedRevocation::Direct(revocation)));
    }
    Ok(load_pending_unlocked(config_dir)?
        .into_iter()
        .next()
        .map(PersistedRevocation::Pending))
}

pub(super) fn complete_pending(
    config_dir: &str,
    revocation: &PendingRevocation,
) -> anyhow::Result<()> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _login_guard = LOGIN_FILE_LOCK.lock().expect("Studio login file lock");
    let _pending_guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    completed::record_unlocked(config_dir, revocation)?;
    if load_login_unlocked(config_dir)?
        .as_ref()
        .is_some_and(|login| matches_login(login, revocation))
    {
        clear_login_unlocked(config_dir)?
            .require_confirmed("durably clear completed-revocation Studio login")?;
    }
    let mut pending = load_pending_unlocked(config_dir)?;
    pending.retain(|candidate| candidate != revocation);
    store_pending(config_dir, &pending)?
        .report("pandar pending revocation completion durability warning");
    Ok(())
}

fn mutate_pending(
    config_dir: &str,
    mutate: impl FnOnce(&mut Vec<PendingRevocation>) -> bool,
) -> anyhow::Result<MutationDurability> {
    let _process_guard = process_lock::acquire(config_dir)?;
    let _guard = PENDING_QUEUE_LOCK
        .lock()
        .expect("pending revocation queue lock");
    let mut pending = load_pending_unlocked(config_dir)?;
    if mutate(&mut pending) {
        return store_pending(config_dir, &pending);
    }
    Ok(MutationDurability::Confirmed)
}

fn store_pending(
    config_dir: &str,
    pending: &[PendingRevocation],
) -> anyhow::Result<MutationDurability> {
    if config_dir.is_empty() {
        return Ok(MutationDurability::Confirmed);
    }
    let directory = Path::new(config_dir);
    fs::create_dir_all(directory).context("create pending plugin revocation directory")?;
    let path = directory.join(PENDING_REVOCATIONS_FILE);
    let durability = if pending.is_empty() {
        durable::remove(&path, directory, "remove pending plugin revocations")?
    } else {
        let body = serde_json::to_vec(pending).context("encode pending plugin revocations")?;
        durable::write_replace(directory, &path, &body)?
    };
    Ok(durability.reconfirm(directory, "confirm pending plugin revocation update"))
}

#[cfg(test)]
fn load_after_login_lock(
    config_dir: &str,
    after_login_lock: impl FnOnce(),
) -> anyhow::Result<Option<PersistedLogin>> {
    load_snapshot(config_dir, after_login_lock)
}

pub(super) fn prepare_direct(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<MutationDurability> {
    direct::prepare_login(config_dir, candidate)
}

pub(super) fn prepare_orphan_direct(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<MutationDurability> {
    direct::prepare_orphan(config_dir, candidate)
}

#[cfg(test)]
pub(super) fn load_direct(config_dir: &str) -> anyhow::Result<Option<PendingRevocation>> {
    direct::load(config_dir)
}

pub(super) fn confirm(config_dir: &str) -> anyhow::Result<()> {
    ensure!(
        !config_dir.is_empty(),
        "persisted account state has no Studio config directory"
    );
    let _process_guard = process_lock::acquire(config_dir)?;
    durable::confirm(Path::new(config_dir))
}

pub(super) fn complete_direct(
    config_dir: &str,
    candidate: &PendingRevocation,
) -> anyhow::Result<()> {
    direct::complete(config_dir, candidate)
}

#[cfg(test)]
mod tests;
