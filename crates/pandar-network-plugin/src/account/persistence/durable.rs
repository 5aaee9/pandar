use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Context;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::account) enum FaultPoint {
    WritePublish,
    RemovalPublish,
    Rollback,
    Cleanup,
}

#[cfg(test)]
thread_local! {
    static FAULTS: std::cell::RefCell<std::collections::VecDeque<FaultPoint>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

#[cfg(test)]
pub(in crate::account) fn fail_next(points: &[FaultPoint]) {
    FAULTS.with(|faults| faults.borrow_mut().extend(points.iter().copied()));
}

#[derive(Debug)]
pub(in crate::account) enum MutationDurability {
    Confirmed,
    ChangedUnconfirmed(anyhow::Error),
}

impl MutationDurability {
    pub(in crate::account) fn report(self, operation: &str) {
        if let Self::ChangedUnconfirmed(error) = self {
            eprintln!("{operation}: change published but durability was not confirmed: {error:#}");
        }
    }

    pub(in crate::account) fn require_confirmed(
        self,
        operation: &'static str,
    ) -> anyhow::Result<()> {
        match self {
            Self::Confirmed => Ok(()),
            Self::ChangedUnconfirmed(error) => Err(error).context(operation),
        }
    }

    pub(super) fn reconfirm(self, directory: &Path, operation: &'static str) -> Self {
        match self {
            Self::Confirmed => Self::Confirmed,
            Self::ChangedUnconfirmed(error) => match confirm(directory) {
                Ok(()) => Self::Confirmed,
                Err(confirm) => Self::ChangedUnconfirmed(error.context(format!(
                    "{operation}; repeated directory durability confirmation failed: {confirm:#}"
                ))),
            },
        }
    }
}

pub(super) fn write_replace(
    directory: &Path,
    target: &Path,
    body: &[u8],
) -> anyhow::Result<MutationDurability> {
    let filename = target
        .file_name()
        .context("persisted target has no filename")?
        .to_string_lossy();
    let temp = temp_path(directory, &filename);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temp)
            .context("create temporary persisted account file")?;
        file.write_all(body)
            .context("write temporary persisted account file")?;
        file.sync_all()
            .context("sync temporary persisted account file")?;
        drop(file);
        atomic_replace(&temp, target).context("atomically replace persisted Studio login")
    })();
    if let Err(error) = result {
        return Err(cleanup_after_failure(&temp, error));
    }
    Ok(
        match sync_directory_at(directory, FaultPointName::Publish) {
            Ok(()) => MutationDurability::Confirmed,
            Err(error) => MutationDurability::ChangedUnconfirmed(
                error.context("confirm persisted account file replacement"),
            ),
        },
    )
}

pub(super) fn remove(
    path: &Path,
    directory: &Path,
    operation: &'static str,
) -> anyhow::Result<MutationDurability> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => anyhow::bail!("{operation}: persisted path is not a regular file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MutationDurability::Confirmed);
        }
        Err(error) => return Err(error).context(operation),
    }

    let filename = path
        .file_name()
        .context("durable removal path has no filename")?
        .to_string_lossy();
    let retired = temp_path(directory, &format!("{filename}.removed"));
    move_path(path, &retired, false).with_context(|| operation)?;

    let mut uncertainty = None;
    if let Err(confirm) = sync_directory_at(directory, FaultPointName::RemovePublish) {
        match move_path(&retired, path, false) {
            Ok(()) => match sync_directory_at(directory, FaultPointName::RemoveRollback) {
                Ok(()) => return Err(confirm).context(operation),
                Err(rollback) => {
                    if let Err(republish) = move_path(path, &retired, false) {
                        return Err(confirm.context(format!(
                            "{operation}; confirm restored file failed: {rollback:#}; remove restored file again failed: {republish:#}"
                        )));
                    }
                    uncertainty = Some(match sync_directory_at(
                        directory,
                        FaultPointName::RemovePublish,
                    ) {
                        Ok(()) => confirm.context(format!(
                            "{operation}; confirm restored file failed before re-removal: {rollback:#}"
                        )),
                        Err(republish) => confirm.context(format!(
                            "{operation}; confirm restored file failed: {rollback:#}; confirm re-removal failed: {republish:#}"
                        )),
                    });
                }
            },
            Err(rollback) => {
                return Ok(MutationDurability::ChangedUnconfirmed(confirm.context(
                    format!("{operation}; restore removed file failed: {rollback:#}"),
                )));
            }
        }
    }

    match fs::remove_file(&retired) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MutationDurability::Confirmed);
        }
        Err(error) => {
            let cleanup =
                anyhow::Error::new(error).context(format!("{operation}; cleanup removed file"));
            return Ok(MutationDurability::ChangedUnconfirmed(match uncertainty {
                Some(error) => error.context(format!("removed-file cleanup failed: {cleanup:#}")),
                None => cleanup,
            }));
        }
    }
    let cleanup = sync_directory_at(directory, FaultPointName::RemoveCleanup);
    Ok(match (uncertainty, cleanup) {
        (None, Ok(())) => MutationDurability::Confirmed,
        (Some(error), Ok(())) => MutationDurability::ChangedUnconfirmed(error),
        (None, Err(error)) => MutationDurability::ChangedUnconfirmed(
            error.context(format!("{operation}; confirm removed-file cleanup")),
        ),
        (Some(error), Err(cleanup)) => MutationDurability::ChangedUnconfirmed(error.context(
            format!("{operation}; confirm removed-file cleanup failed: {cleanup:#}"),
        )),
    })
}

pub(super) fn confirm(directory: &Path) -> anyhow::Result<()> {
    sync_directory_at(directory, FaultPointName::Publish)
        .context("confirm persisted account directory")
}

#[derive(Clone, Copy)]
enum FaultPointName {
    Publish,
    RemovePublish,
    RemoveRollback,
    RemoveCleanup,
}

fn sync_directory_at(directory: &Path, _point: FaultPointName) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        let expected = match _point {
            FaultPointName::Publish => FaultPoint::WritePublish,
            FaultPointName::RemovePublish => FaultPoint::RemovalPublish,
            FaultPointName::RemoveRollback => FaultPoint::Rollback,
            FaultPointName::RemoveCleanup => FaultPoint::Cleanup,
        };
        let fail = FAULTS.with(|faults| {
            let mut faults = faults.borrow_mut();
            if faults.front() == Some(&expected) {
                faults.pop_front();
                true
            } else {
                false
            }
        });
        if fail {
            anyhow::bail!("injected persisted account directory sync failure");
        }
    }
    sync_directory(directory)
}

fn temp_path(directory: &Path, filename: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".{filename}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

fn cleanup_after_failure(temp: &Path, error: anyhow::Error) -> anyhow::Error {
    match fs::remove_file(temp) {
        Ok(()) => error,
        Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => error,
        Err(cleanup) => error.context(format!("cleanup temporary account file failed: {cleanup}")),
    }
}

#[cfg(not(windows))]
fn atomic_replace(temp: &Path, target: &Path) -> anyhow::Result<()> {
    fs::rename(temp, target).context("rename temporary account file")
}

#[cfg(windows)]
fn atomic_replace(temp: &Path, target: &Path) -> anyhow::Result<()> {
    move_file_write_through(temp, target, true).context("replace persisted account file")
}

#[cfg(not(windows))]
fn move_path(existing: &Path, replacement: &Path, _: bool) -> anyhow::Result<()> {
    if replacement.exists() {
        anyhow::bail!("replacement path already exists");
    }
    fs::rename(existing, replacement).context("rename persisted account file")
}

#[cfg(windows)]
fn move_path(existing: &Path, replacement: &Path, replace: bool) -> anyhow::Result<()> {
    move_file_write_through(existing, replacement, replace).map_err(anyhow::Error::new)
}

#[cfg(windows)]
fn move_file_write_through(
    existing: &Path,
    replacement: &Path,
    replace_existing: bool,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let existing = existing
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replacement = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace_existing {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    let moved = unsafe { MoveFileExW(existing.as_ptr(), replacement.as_ptr(), flags) };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> anyhow::Result<()> {
    std::fs::File::open(directory)
        .context("open persisted account directory for sync")?
        .sync_all()
        .context("sync persisted account directory")
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_write_sync_failure_is_not_reported_as_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("login.json");
        fail_next(&[FaultPoint::WritePublish]);

        let outcome = write_replace(directory.path(), &target, b"new-login").unwrap();

        assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
        assert_eq!(fs::read(target).unwrap(), b"new-login");
    }

    #[test]
    fn removal_sync_failure_rolls_back_before_returning_error() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("login.json");
        fs::write(&target, b"original-login").unwrap();
        fail_next(&[FaultPoint::RemovalPublish]);

        let error = remove(&target, directory.path(), "remove test login").unwrap_err();

        assert!(error.to_string().contains("remove test login"));
        assert_eq!(fs::read(target).unwrap(), b"original-login");
    }

    #[test]
    fn unconfirmed_removal_rollback_is_not_reported_as_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("login.json");
        fs::write(&target, b"original-login").unwrap();
        fail_next(&[FaultPoint::RemovalPublish, FaultPoint::Rollback]);

        let outcome = remove(&target, directory.path(), "remove test login").unwrap();

        assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
        assert!(!target.exists());
    }

    #[test]
    fn cleanup_sync_failure_keeps_canonical_path_removed() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("login.json");
        fs::write(&target, b"original-login").unwrap();
        fail_next(&[FaultPoint::Cleanup]);

        let outcome = remove(&target, directory.path(), "remove test login").unwrap();

        assert!(matches!(outcome, MutationDurability::ChangedUnconfirmed(_)));
        assert!(!target.exists());
    }
}
