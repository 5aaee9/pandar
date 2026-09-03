use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::{Mutex, MutexGuard},
};

use anyhow::Context;

static PROCESS_LOCK: Mutex<()> = Mutex::new(());
const LOCK_FILE: &str = ".pandar-plugin-account.lock";

pub(in crate::account) struct AccountFileLock {
    file: File,
    _process: MutexGuard<'static, ()>,
}

pub(in crate::account) fn acquire(config_dir: &str) -> anyhow::Result<Option<AccountFileLock>> {
    if config_dir.is_empty() {
        return Ok(None);
    }
    let process = PROCESS_LOCK.lock().expect("Studio account process lock");
    let directory = Path::new(config_dir);
    std::fs::create_dir_all(directory).context("create Studio account state directory")?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let file = options
        .open(directory.join(LOCK_FILE))
        .context("open Studio account process lock")?;
    platform::lock(&file).context("lock Studio account state across processes")?;
    Ok(Some(AccountFileLock {
        file,
        _process: process,
    }))
}

impl Drop for AccountFileLock {
    fn drop(&mut self) {
        if let Err(error) = platform::unlock(&self.file) {
            eprintln!("pandar Studio account process unlock failed: {error}");
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::{io, os::fd::AsRawFd};

    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    const LOCK_EX: i32 = 2;
    const LOCK_UN: i32 = 8;

    pub(super) fn lock(file: &std::fs::File) -> io::Result<()> {
        call(file, LOCK_EX)
    }

    pub(super) fn unlock(file: &std::fs::File) -> io::Result<()> {
        call(file, LOCK_UN)
    }

    fn call(file: &std::fs::File, operation: i32) -> io::Result<()> {
        if unsafe { flock(file.as_raw_fd(), operation) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, io, os::windows::io::AsRawHandle};

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut c_void,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LockFileEx(
            file: *mut c_void,
            flags: u32,
            reserved: u32,
            bytes_low: u32,
            bytes_high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn UnlockFileEx(
            file: *mut c_void,
            reserved: u32,
            bytes_low: u32,
            bytes_high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
    }

    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x2;

    pub(super) fn lock(file: &std::fs::File) -> io::Result<()> {
        let mut overlapped = empty_overlapped();
        let status = unsafe {
            LockFileEx(
                file.as_raw_handle(),
                LOCKFILE_EXCLUSIVE_LOCK,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlapped,
            )
        };
        result(status)
    }

    pub(super) fn unlock(file: &std::fs::File) -> io::Result<()> {
        let mut overlapped = empty_overlapped();
        let status =
            unsafe { UnlockFileEx(file.as_raw_handle(), 0, u32::MAX, u32::MAX, &mut overlapped) };
        result(status)
    }

    fn empty_overlapped() -> Overlapped {
        Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: std::ptr::null_mut(),
        }
    }

    fn result(status: i32) -> io::Result<()> {
        if status != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{process::Command, time::Duration};

    use super::*;

    const CHILD_CONFIG: &str = "PANDAR_PROCESS_LOCK_CHILD_CONFIG";

    #[test]
    fn another_process_waits_for_the_account_file_lock() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().to_string_lossy().into_owned();
        let guard = acquire(&config_dir).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "account::persistence::process_lock::tests::child_waits_for_parent_lock",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_CONFIG, &config_dir)
            .spawn()
            .unwrap();
        let attempted = directory.path().join("child-lock-attempted");
        let acquired = directory.path().join("child-lock-acquired");
        for _ in 0..500 {
            if attempted.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(attempted.exists());
        assert!(!acquired.exists());

        drop(guard);

        assert!(child.wait().unwrap().success());
        assert!(acquired.exists());
    }

    #[test]
    #[ignore = "subprocess helper"]
    fn child_waits_for_parent_lock() {
        let Some(config_dir) = std::env::var_os(CHILD_CONFIG) else {
            return;
        };
        let directory = Path::new(&config_dir);
        std::fs::write(directory.join("child-lock-attempted"), b"attempted\n").unwrap();
        let _guard = acquire(directory.to_str().unwrap()).unwrap();
        std::fs::write(directory.join("child-lock-acquired"), b"acquired\n").unwrap();
    }
}
