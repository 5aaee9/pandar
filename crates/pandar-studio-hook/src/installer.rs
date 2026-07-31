use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use pandar_network_plugin::installer::{InstallNetworkPluginOptions, install_network_plugin};
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::release::{StudioHookRelease, download_latest_studio_hook_release};

const PROXY_DLL: &str = "swscale-8.dll";
const ORIGINAL_DLL: &str = "swscale8original.dll";
const PLUGIN_PACKAGE: &str = "networking_plugins.zip";
const HOOK_DATA_DIR: &str = "Pandar/studio-hook";

#[derive(Debug)]
pub struct InstallStudioHookOptions {
    pub studio_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct UninstallStudioHookOptions {
    pub studio_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StudioHookSummary {
    pub studio_dir: PathBuf,
    pub proxy_path: PathBuf,
    pub original_path: PathBuf,
    pub plugin_path: PathBuf,
    pub source_path: PathBuf,
    pub config_path: PathBuf,
    pub plugin_package_path: PathBuf,
}

pub async fn install_studio_hook(
    options: InstallStudioHookOptions,
) -> anyhow::Result<StudioHookSummary> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        bail!("Bambu Studio hook installation is only supported on Windows x86-64");
    }
    let release = download_latest_studio_hook_release().await?;
    install_studio_hook_release(options, &release)
}

fn install_studio_hook_release(
    options: InstallStudioHookOptions,
    release: &StudioHookRelease,
) -> anyhow::Result<StudioHookSummary> {
    let studio_dir = resolve_studio_dir(options.studio_dir)?;
    let data_dir = resolve_data_dir(options.data_dir)?;
    let hook_data_dir = resolve_hook_data_dir()?;
    let plugin_package_path =
        write_plugin_package(&hook_data_dir, &release.plugin_file, &release.source_file)?;
    let network = install_network_plugin(InstallNetworkPluginOptions {
        plugin_file: release.plugin_file.clone(),
        source_file: release.source_file.clone(),
        data_dir: Some(data_dir),
    })?;
    let (proxy_path, original_path) = install_proxy(&studio_dir, &release.hook_file)?;

    Ok(StudioHookSummary {
        studio_dir,
        proxy_path,
        original_path,
        plugin_path: network.plugin_path,
        source_path: network.source_path,
        config_path: network.config_path,
        plugin_package_path,
    })
}

pub fn uninstall_studio_hook(
    options: UninstallStudioHookOptions,
) -> anyhow::Result<StudioHookSummary> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        bail!("Bambu Studio hook uninstallation is only supported on Windows x86-64");
    }

    let studio_dir = resolve_studio_dir(options.studio_dir)?;
    let data_dir = data_dir_path(options.data_dir)?;
    let plugin_package_path = resolve_hook_data_dir()?.join(PLUGIN_PACKAGE);
    let (proxy_path, original_path) = restore_proxy(&studio_dir)?;

    if plugin_package_path.exists() {
        fs::remove_file(&plugin_package_path).with_context(|| {
            format!(
                "remove cached Pandar Studio plugin package {}",
                plugin_package_path.display()
            )
        })?;
    }

    Ok(StudioHookSummary {
        studio_dir,
        proxy_path,
        original_path,
        plugin_path: data_dir.join("plugins").join("bambu_networking.dll"),
        source_path: data_dir.join("plugins").join("BambuSource.dll"),
        config_path: data_dir.join("BambuStudio.conf"),
        plugin_package_path,
    })
}

fn install_proxy(studio_dir: &Path, hook_file: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
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

    fs::copy(hook_file, &proxy_path).with_context(|| {
        format!(
            "install Studio hook from {} to {}",
            hook_file.display(),
            proxy_path.display()
        )
    })?;
    Ok((proxy_path, original_path))
}

fn restore_proxy(studio_dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
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
    Ok((proxy_path, original_path))
}

fn write_plugin_package(
    hook_data_dir: &Path,
    plugin_file: &Path,
    source_file: &Path,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(hook_data_dir).with_context(|| {
        format!(
            "create Studio hook data directory {}",
            hook_data_dir.display()
        )
    })?;
    let package_path = hook_data_dir.join(PLUGIN_PACKAGE);
    let temporary_path = hook_data_dir.join(format!("{PLUGIN_PACKAGE}.tmp"));
    let file = fs::File::create(&temporary_path).with_context(|| {
        format!(
            "create temporary Pandar Studio plugin package {}",
            temporary_path.display()
        )
    })?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    add_package_file(&mut archive, "bambu_networking.dll", plugin_file, options)?;
    add_package_file(&mut archive, "BambuSource.dll", source_file, options)?;
    archive
        .finish()
        .context("finish Pandar Studio plugin package")?
        .sync_all()
        .context("flush Pandar Studio plugin package")?;
    if package_path.exists() {
        fs::remove_file(&package_path).with_context(|| {
            format!(
                "remove previous Pandar Studio plugin package {}",
                package_path.display()
            )
        })?;
    }
    fs::rename(&temporary_path, &package_path).with_context(|| {
        format!(
            "publish Pandar Studio plugin package from {} to {}",
            temporary_path.display(),
            package_path.display()
        )
    })?;
    Ok(package_path)
}

fn add_package_file(
    archive: &mut ZipWriter<fs::File>,
    name: &str,
    source: &Path,
    options: SimpleFileOptions,
) -> anyhow::Result<()> {
    archive
        .start_file(name, options)
        .with_context(|| format!("start Studio plugin package member {name}"))?;
    let mut input = fs::File::open(source)
        .with_context(|| format!("open Studio plugin package input {}", source.display()))?;
    io::copy(&mut input, archive)
        .with_context(|| format!("write Studio plugin package member {name}"))?;
    archive
        .flush()
        .context("flush Studio plugin package member")?;
    Ok(())
}

fn resolve_studio_dir(studio_dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let studio_dir = studio_dir.unwrap_or_else(default_studio_dir);
    let proxy_path = studio_dir.join(PROXY_DLL);
    if !proxy_path.is_file() {
        bail!("Bambu Studio DLL does not exist: {}", proxy_path.display());
    }
    Ok(studio_dir)
}

fn resolve_data_dir(data_dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let data_dir = data_dir_path(data_dir)?;
    if !data_dir.join("BambuStudio.conf").is_file() {
        bail!(
            "BambuStudio.conf not found at {}",
            data_dir.join("BambuStudio.conf").display()
        );
    }
    Ok(data_dir)
}

fn data_dir_path(data_dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match data_dir {
        Some(path) => Ok(path),
        None => {
            let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
            Ok(PathBuf::from(appdata).join("BambuStudio"))
        }
    }
}

fn resolve_hook_data_dir() -> anyhow::Result<PathBuf> {
    let local_appdata = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local_appdata).join(HOOK_DATA_DIR))
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
    fn creates_studio_shaped_plugin_package() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("BambuStudio");
        fs::create_dir(&data_dir).unwrap();
        let plugin = temp.path().join("pandar_network_plugin.dll");
        let source = temp.path().join("pandar_bambu_source.dll");
        fs::write(&plugin, b"plugin").unwrap();
        fs::write(&source, b"source").unwrap();

        let package = write_plugin_package(&data_dir, &plugin, &source).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(package).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut plugin_body = Vec::new();
        io::Read::read_to_end(
            &mut archive.by_name("bambu_networking.dll").unwrap(),
            &mut plugin_body,
        )
        .unwrap();
        assert_eq!(plugin_body, b"plugin");
        let mut source_body = Vec::new();
        io::Read::read_to_end(
            &mut archive.by_name("BambuSource.dll").unwrap(),
            &mut source_body,
        )
        .unwrap();
        assert_eq!(source_body, b"source");
    }

    #[test]
    fn installs_and_restores_swscale_proxy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let studio_dir = temp.path().join("Bambu Studio");
        fs::create_dir_all(&studio_dir).expect("create studio dir");
        fs::write(studio_dir.join(PROXY_DLL), b"original").expect("write original");
        let hook_file = temp.path().join("pandar_studio_hook.dll");
        fs::write(&hook_file, b"hook").unwrap();

        let (proxy_path, original_path) = install_proxy(&studio_dir, &hook_file).unwrap();
        assert_eq!(fs::read(&proxy_path).unwrap(), b"hook");
        assert_eq!(fs::read(&original_path).unwrap(), b"original");

        restore_proxy(&studio_dir).unwrap();
        assert_eq!(fs::read(proxy_path).unwrap(), b"original");
        assert!(!original_path.exists());
    }
}
