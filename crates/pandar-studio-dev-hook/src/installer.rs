use anyhow::{Context, bail};
use std::{fs, path::PathBuf};

const PROXY_DLL: &str = "swscale-8.dll";
const ORIGINAL_DLL: &str = "swscale8original.dll";

#[derive(Debug)]
pub struct InstallStudioDevHookOptions {
    pub hook_file: PathBuf,
    pub studio_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct UninstallStudioDevHookOptions {
    pub studio_dir: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StudioDevHookSummary {
    pub studio_dir: PathBuf,
    pub proxy_path: PathBuf,
    pub original_path: PathBuf,
}

pub fn install_studio_dev_hook(
    options: InstallStudioDevHookOptions,
) -> anyhow::Result<StudioDevHookSummary> {
    if !cfg!(windows) {
        bail!("Bambu Studio dev hook installation is only supported on Windows");
    }
    if !options.hook_file.is_file() {
        bail!(
            "Studio dev hook file does not exist: {}",
            options.hook_file.display()
        );
    }

    let studio_dir = resolve_studio_dir(options.studio_dir)?;
    let proxy_path = studio_dir.join(PROXY_DLL);
    let original_path = studio_dir.join(ORIGINAL_DLL);

    if !original_path.exists() {
        fs::copy(&proxy_path, &original_path).with_context(|| {
            format!(
                "backup original Bambu Studio DLL from {} to {}",
                proxy_path.display(),
                original_path.display()
            )
        })?;
    }

    fs::copy(&options.hook_file, &proxy_path).with_context(|| {
        format!(
            "install Studio dev hook from {} to {}",
            options.hook_file.display(),
            proxy_path.display()
        )
    })?;

    Ok(StudioDevHookSummary {
        studio_dir,
        proxy_path,
        original_path,
    })
}

pub fn uninstall_studio_dev_hook(
    options: UninstallStudioDevHookOptions,
) -> anyhow::Result<StudioDevHookSummary> {
    if !cfg!(windows) {
        bail!("Bambu Studio dev hook uninstallation is only supported on Windows");
    }

    let studio_dir = resolve_studio_dir(options.studio_dir)?;
    let proxy_path = studio_dir.join(PROXY_DLL);
    let original_path = studio_dir.join(ORIGINAL_DLL);
    if !original_path.is_file() {
        bail!(
            "original Bambu Studio DLL backup does not exist: {}",
            original_path.display()
        );
    }

    fs::copy(&original_path, &proxy_path).with_context(|| {
        format!(
            "restore original Bambu Studio DLL from {} to {}",
            original_path.display(),
            proxy_path.display()
        )
    })?;
    fs::remove_file(&original_path)
        .with_context(|| format!("remove original DLL backup {}", original_path.display()))?;

    Ok(StudioDevHookSummary {
        studio_dir,
        proxy_path,
        original_path,
    })
}

fn resolve_studio_dir(studio_dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let studio_dir = studio_dir.unwrap_or_else(default_studio_dir);
    let proxy_path = studio_dir.join(PROXY_DLL);
    if !proxy_path.is_file() {
        bail!("Bambu Studio DLL does not exist: {}", proxy_path.display());
    }
    Ok(studio_dir)
}

fn default_studio_dir() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Program Files"))
        .join("Bambu Studio")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_and_restores_swscale_proxy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let studio_dir = temp.path().join("Bambu Studio");
        fs::create_dir_all(&studio_dir).expect("create studio dir");
        fs::write(studio_dir.join(PROXY_DLL), b"original").expect("write original");
        let hook_file = temp.path().join("pandar_studio_dev_hook.dll");
        fs::write(&hook_file, b"hook").expect("write hook");

        let summary = install_studio_dev_hook(InstallStudioDevHookOptions {
            hook_file,
            studio_dir: Some(studio_dir.clone()),
        })
        .expect("install hook");

        assert_eq!(summary.proxy_path, studio_dir.join(PROXY_DLL));
        assert_eq!(summary.original_path, studio_dir.join(ORIGINAL_DLL));
        assert_eq!(
            fs::read(studio_dir.join(PROXY_DLL)).expect("read proxy"),
            b"hook"
        );
        assert_eq!(
            fs::read(studio_dir.join(ORIGINAL_DLL)).expect("read backup"),
            b"original"
        );

        uninstall_studio_dev_hook(UninstallStudioDevHookOptions {
            studio_dir: Some(studio_dir.clone()),
        })
        .expect("uninstall hook");

        assert_eq!(
            fs::read(studio_dir.join(PROXY_DLL)).expect("read restored"),
            b"original"
        );
        assert!(!studio_dir.join(ORIGINAL_DLL).exists());
    }
}
