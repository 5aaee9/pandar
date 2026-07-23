use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

pub(crate) const PLUGIN_OVERRIDE_ENV: &str = "PANDAR_STUDIO_PRINT_CONTRACT_PLUGIN";

pub(super) struct PluginIdentity {
    pub(super) path: PathBuf,
    pub(super) sha256: String,
}

pub(super) struct SelectedLibrary {
    pub(super) identity: PluginIdentity,
    pub(super) source: &'static str,
}

fn plugin_identity(path: &Path) -> Result<PluginIdentity, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("resolve plugin {}: {error}", path.display()))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("inspect plugin {}: {error}", canonical.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "plugin is not a regular file: {}",
            canonical.display()
        ));
    }
    let contents = fs::read(&canonical)
        .map_err(|error| format!("read plugin {}: {error}", canonical.display()))?;
    Ok(PluginIdentity {
        path: canonical,
        sha256: format!("{:x}", Sha256::digest(contents)),
    })
}

pub(super) fn select_library(
    configured: Option<&OsStr>,
    build_debug: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<SelectedLibrary, String> {
    let (path, source) = match configured {
        Some(configured) => (PathBuf::from(configured), "override"),
        None => (build_debug()?, "debug-build"),
    };
    Ok(SelectedLibrary {
        identity: plugin_identity(&path)?,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{plugin_identity, select_library};
    use std::fs;

    #[test]
    fn configured_plugin_identity_reports_canonical_path_and_sha256() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let plugin = directory.path().join("packaged-plugin");
        fs::write(&plugin, b"packaged plugin bytes").unwrap();
        let configured = nested.join("..").join("packaged-plugin");

        let identity = plugin_identity(&configured).unwrap();

        assert_eq!(identity.path, fs::canonicalize(plugin).unwrap());
        assert_eq!(
            identity.sha256,
            "1d457f98b5729cadf2a3e3de5d975147c12a5c20b58212455e92e17db20a79d0"
        );
    }

    #[test]
    fn configured_plugin_must_be_a_regular_file() {
        let directory = tempfile::tempdir().unwrap();

        let Err(error) = plugin_identity(directory.path()) else {
            panic!("directory was accepted as a plugin");
        };

        assert!(
            error.contains("plugin is not a regular file"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn configured_plugin_selection_skips_debug_build_and_reports_identity() {
        let directory = tempfile::tempdir().unwrap();
        let plugin = directory.path().join("packaged-plugin");
        fs::write(&plugin, b"packaged plugin bytes").unwrap();

        let selected = select_library(Some(plugin.as_os_str()), || {
            panic!("debug plugin build must not run when an override is configured")
        })
        .unwrap();

        assert_eq!(selected.source, "override");
        assert_eq!(selected.identity.path, fs::canonicalize(plugin).unwrap());
        assert_eq!(
            selected.identity.sha256,
            "1d457f98b5729cadf2a3e3de5d975147c12a5c20b58212455e92e17db20a79d0"
        );
    }
}
