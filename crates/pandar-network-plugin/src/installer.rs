use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone)]
pub struct InstallNetworkPluginOptions {
    pub plugin_file: PathBuf,
    pub source_file: PathBuf,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallNetworkPluginSummary {
    pub plugin_path: PathBuf,
    pub source_path: PathBuf,
    pub config_path: PathBuf,
}

pub fn install_network_plugin(
    options: InstallNetworkPluginOptions,
) -> anyhow::Result<InstallNetworkPluginSummary> {
    if !options.plugin_file.is_file() {
        bail!(
            "network plugin file does not exist: {}",
            options.plugin_file.display()
        );
    }
    if !options.source_file.is_file() {
        bail!(
            "BambuSource companion file does not exist: {}",
            options.source_file.display()
        );
    }
    let data_dir = match options.data_dir {
        Some(path) => path,
        None => default_bambu_studio_data_dir()?,
    };
    let config_path = data_dir.join("BambuStudio.conf");
    if !config_path.is_file() {
        bail!("BambuStudio.conf not found at {}", config_path.display());
    }

    let plugins_dir = data_dir.join("plugins");
    fs::create_dir_all(&plugins_dir)
        .with_context(|| format!("create plugins directory {}", plugins_dir.display()))?;
    let plugin_path = plugins_dir.join(bambu_network_plugin_filename());
    fs::copy(&options.plugin_file, &plugin_path).with_context(|| {
        format!(
            "copy network plugin from {} to {}",
            options.plugin_file.display(),
            plugin_path.display()
        )
    })?;
    let source_path = plugins_dir.join(bambu_source_filename());
    fs::copy(&options.source_file, &source_path).with_context(|| {
        format!(
            "copy BambuSource companion from {} to {}",
            options.source_file.display(),
            source_path.display()
        )
    })?;

    patch_bambu_studio_config(&config_path)?;

    Ok(InstallNetworkPluginSummary {
        plugin_path,
        source_path,
        config_path,
    })
}

fn bambu_network_plugin_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "bambu_networking.dll"
    } else if cfg!(target_os = "macos") {
        "libbambu_networking.dylib"
    } else {
        "libbambu_networking.so"
    }
}

fn bambu_source_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "BambuSource.dll"
    } else if cfg!(target_os = "macos") {
        "libBambuSource.dylib"
    } else {
        "libBambuSource.so"
    }
}

fn default_bambu_studio_data_dir() -> anyhow::Result<PathBuf> {
    if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
        Ok(PathBuf::from(appdata).join("BambuStudio"))
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("BambuStudio"))
    } else {
        let home = std::env::var_os("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join(".config").join("BambuStudio"))
    }
}

fn patch_bambu_studio_config(path: &Path) -> anyhow::Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    let json_body = strip_md5_checksum(&raw);
    let mut config: BambuStudioConfig = serde_json::from_str(json_body)
        .with_context(|| format!("parse config JSON {}", path.display()))?;

    config.app.installed_networking = "1".to_owned();
    config.app.update_network_plugin = Some(BambuStudioConfigValue::Bool(false));
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        config.app.ignore_module_cert = Some(BambuStudioConfigValue::Bool(true));
    }

    let backup_path = path.with_extension("conf.pandar-bak");
    fs::copy(path, &backup_path).with_context(|| {
        format!(
            "backup config from {} to {}",
            path.display(),
            backup_path.display()
        )
    })?;
    let patched = format!(
        "{}\n# MD5 checksum 00000000000000000000000000000000\n",
        serde_json::to_string_pretty(&config)?
    );
    fs::write(path, patched).with_context(|| format!("write config {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct BambuStudioConfig {
    app: BambuStudioAppConfig,
    #[serde(flatten)]
    extra: BTreeMap<String, BambuStudioConfigValue>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BambuStudioAppConfig {
    #[serde(default)]
    installed_networking: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    update_network_plugin: Option<BambuStudioConfigValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ignore_module_cert: Option<BambuStudioConfigValue>,
    #[serde(flatten)]
    extra: BTreeMap<String, BambuStudioConfigValue>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum BambuStudioConfigValue {
    Object(BTreeMap<String, BambuStudioConfigValue>),
    Array(Vec<BambuStudioConfigValue>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

fn strip_md5_checksum(raw: &str) -> &str {
    raw.split_once("\n# MD5 checksum")
        .map_or(raw.trim(), |(json, _)| json.trim())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestBambuStudioConfig {
        app: TestBambuStudioAppConfig,
    }

    #[derive(Debug, Deserialize)]
    struct TestBambuStudioAppConfig {
        installed_networking: String,
        update_network_plugin: bool,
        ignore_module_cert: Option<bool>,
    }

    #[test]
    fn installs_specified_file_as_bambu_studio_network_plugin() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let data_dir = temp.path().join("BambuStudio");
        fs::create_dir_all(&data_dir).expect("create data dir");
        let original_config = r#"{"app":{"version":"02.06.00.51","installed_networking":"0"}}"#;
        fs::write(data_dir.join("BambuStudio.conf"), original_config).expect("write config");
        let plugin_file = temp.path().join("pandar_network_plugin.dll");
        fs::write(&plugin_file, b"plugin bytes").expect("write plugin");
        let source_file = temp.path().join("pandar_bambu_source.dll");
        fs::write(&source_file, b"source bytes").expect("write source companion");

        let summary = install_network_plugin(InstallNetworkPluginOptions {
            plugin_file: plugin_file.clone(),
            source_file: source_file.clone(),
            data_dir: Some(data_dir.clone()),
        })
        .expect("install plugin");

        let expected_plugin_path = data_dir
            .join("plugins")
            .join(bambu_network_plugin_filename());
        assert_eq!(summary.plugin_path, expected_plugin_path);
        let expected_source_path = data_dir.join("plugins").join(bambu_source_filename());
        assert_eq!(summary.source_path, expected_source_path);
        assert_eq!(summary.config_path, data_dir.join("BambuStudio.conf"));
        assert_eq!(
            fs::read(expected_plugin_path).expect("read installed plugin"),
            b"plugin bytes"
        );
        assert_eq!(
            fs::read(expected_source_path).expect("read installed source companion"),
            b"source bytes"
        );
        assert_eq!(
            fs::read_to_string(data_dir.join("BambuStudio.conf.pandar-bak"))
                .expect("read config backup"),
            original_config
        );
    }

    #[test]
    fn patches_bambu_studio_config_for_manual_plugin_installation() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let config_path = temp.path().join("BambuStudio.conf");
        fs::write(
            &config_path,
            r#"{"app":{"version":"02.06.00.51","installed_networking":"0","update_network_plugin":"true"}}"#,
        )
        .expect("write config");

        patch_bambu_studio_config(&config_path).expect("patch config");

        let patched = fs::read_to_string(config_path).expect("read config");
        let config: TestBambuStudioConfig = serde_json::from_str(
            patched
                .trim_end()
                .trim_end_matches("# MD5 checksum 00000000000000000000000000000000"),
        )
        .expect("parse patched config");
        assert_eq!(config.app.installed_networking, "1");
        assert!(!config.app.update_network_plugin);
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            assert_eq!(config.app.ignore_module_cert, Some(true));
        } else {
            assert_eq!(config.app.ignore_module_cert, None);
        }
    }

    #[test]
    fn missing_source_companion_is_rejected_before_installation_changes() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let data_dir = temp.path().join("BambuStudio");
        fs::create_dir_all(&data_dir).expect("create data dir");
        let config_path = data_dir.join("BambuStudio.conf");
        let original_config = r#"{"app":{"installed_networking":"0"}}"#;
        fs::write(&config_path, original_config).expect("write config");
        let plugin_file = temp.path().join("pandar_network_plugin.dll");
        fs::write(&plugin_file, b"plugin bytes").expect("write plugin");
        let source_file = temp.path().join("missing-source.dll");

        let error = install_network_plugin(InstallNetworkPluginOptions {
            plugin_file,
            source_file: source_file.clone(),
            data_dir: Some(data_dir.clone()),
        })
        .unwrap_err();

        assert!(format!("{error:#}").contains(&source_file.display().to_string()));
        assert!(!data_dir.join("plugins").exists());
        assert_eq!(fs::read_to_string(config_path).unwrap(), original_config);
    }

    #[test]
    fn patches_mixed_network_plugin_flag_types() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let config_path = temp.path().join("BambuStudio.conf");
        fs::write(
            &config_path,
            r#"{"app":{"installed_networking":"0","update_network_plugin":false,"ignore_module_cert":"1"}}"#,
        )
        .expect("write config");

        patch_bambu_studio_config(&config_path).expect("patch config");

        let patched = fs::read_to_string(config_path).expect("read config");
        let config: serde_json::Value = serde_json::from_str(
            patched
                .trim_end()
                .trim_end_matches("# MD5 checksum 00000000000000000000000000000000"),
        )
        .expect("parse patched config");
        assert_eq!(config["app"]["update_network_plugin"], false);
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            assert_eq!(config["app"]["ignore_module_cert"], true);
        } else {
            assert_eq!(config["app"]["ignore_module_cert"], "1");
        }
    }
}
