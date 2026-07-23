use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;
use tempfile::{Builder, TempDir};

struct ChecksumSidecar {
    digest: String,
    archive_name: String,
}

pub(crate) struct StagedArchive {
    _temp: TempDir,
    pub cli: PathBuf,
    pub plugin: PathBuf,
    pub source: PathBuf,
}

impl StagedArchive {
    #[cfg(test)]
    fn root(&self) -> &Path {
        self._temp.path()
    }
}

pub(crate) fn validate_checksum(archive: &Path, checksum: &Path) -> Result<String, String> {
    let content = fs::read_to_string(checksum)
        .map_err(|error| format!("read checksum sidecar {}: {error}", checksum.display()))?;
    let sidecar = parse_checksum_sidecar(&content)?;
    let archive_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("archive path has no UTF-8 file name: {}", archive.display()))?;
    if sidecar.archive_name != archive_name {
        return Err(format!(
            "checksum sidecar names {}, expected {archive_name}",
            sidecar.archive_name
        ));
    }
    let actual = sha256_hex(archive)?;
    if actual != sidecar.digest.to_ascii_lowercase() {
        return Err(format!(
            "checksum mismatch: expected {}, got {actual}",
            sidecar.digest
        ));
    }
    Ok(actual)
}

pub(crate) fn stage_archive(
    archive: &Path,
    cli_name: &str,
    plugin_name: &str,
    source_name: &str,
) -> Result<StagedArchive, String> {
    validate_artifact_name(cli_name)?;
    validate_artifact_name(plugin_name)?;
    validate_artifact_name(source_name)?;
    let expected_paths = BTreeSet::from([
        PathBuf::from(cli_name),
        PathBuf::from(plugin_name),
        PathBuf::from(source_name),
    ]);
    if expected_paths.len() != 3 {
        return Err("CLI, plugin, and BambuSource artifact names must differ".to_owned());
    }
    let temp = Builder::new()
        .prefix("pandar-release-smoke-")
        .tempdir()
        .map_err(|error| format!("create release smoke stage: {error}"))?;
    unpack_into(archive, temp.path(), &expected_paths)?;
    validate_layout(temp.path(), cli_name, plugin_name, source_name)?;
    let cli = fs::canonicalize(temp.path().join(cli_name))
        .map_err(|error| format!("resolve staged CLI artifact: {error}"))?;
    let plugin = fs::canonicalize(temp.path().join(plugin_name))
        .map_err(|error| format!("resolve staged plugin artifact: {error}"))?;
    let source = fs::canonicalize(temp.path().join(source_name))
        .map_err(|error| format!("resolve staged BambuSource artifact: {error}"))?;
    Ok(StagedArchive {
        _temp: temp,
        cli,
        plugin,
        source,
    })
}

pub(crate) fn sha256_hex(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("open artifact for SHA-256: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read artifact for SHA-256: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_checksum_sidecar(content: &str) -> Result<ChecksumSidecar, String> {
    let mut lines = content.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| "checksum sidecar must contain exactly one non-empty line".to_owned())?;
    if lines.next().is_some() {
        return Err("checksum sidecar must contain exactly one non-empty line".to_owned());
    }
    let mut fields = line.split_whitespace();
    let digest = fields
        .next()
        .ok_or_else(|| "checksum sidecar must contain digest and archive name".to_owned())?;
    let archive_name = fields
        .next()
        .ok_or_else(|| "checksum sidecar must contain digest and archive name".to_owned())?;
    if fields.next().is_some() {
        return Err("checksum sidecar must contain exactly two fields".to_owned());
    }
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("checksum digest must be 64 hex characters".to_owned());
    }
    validate_artifact_name(archive_name)?;
    Ok(ChecksumSidecar {
        digest: digest.to_owned(),
        archive_name: archive_name.to_owned(),
    })
}

fn validate_artifact_name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "artifact name must be one top-level file name: {name}"
        ));
    }
    Ok(())
}

fn unpack_into(
    archive: &Path,
    stage: &Path,
    expected_paths: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    let archive_file = File::open(archive)
        .map_err(|error| format!("open release archive {}: {error}", archive.display()))?;
    let mut archive = Archive::new(GzDecoder::new(archive_file));
    let mut normalized_paths = BTreeSet::new();
    let mut case_folded_paths = BTreeMap::new();
    for entry in archive
        .entries()
        .map_err(|error| format!("read release archive entries: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read release archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("read release archive entry path: {error}"))?;
        let relative = normalized_top_level_path(&path)?.ok_or_else(|| {
            format!(
                "archive entry {} normalizes to an empty path",
                path.display()
            )
        })?;
        if !normalized_paths.insert(relative.clone()) {
            return Err(format!(
                "archive contains duplicate normalized entry: {}",
                relative.display()
            ));
        }
        let case_folded = relative.to_string_lossy().to_ascii_lowercase();
        if let Some(first) = case_folded_paths.insert(case_folded, relative.clone()) {
            return Err(format!(
                "archive contains case-folded normalized entry collision: {} conflicts with {}",
                relative.display(),
                first.display()
            ));
        }
        if !expected_paths.contains(&relative) {
            return Err(format!(
                "archive contains unexpected normalized entry: {}",
                relative.display()
            ));
        }
        if !entry.header().entry_type().is_file() {
            return Err(format!(
                "archive entry {} must be a regular top-level file",
                path.display()
            ));
        }
        entry
            .unpack(stage.join(&relative))
            .map_err(|error| format!("unpack release archive entry: {error}"))?;
    }
    Ok(())
}

fn normalized_top_level_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut normalized = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => normalized.push(name.to_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "archive entry {} contains parent-directory traversal",
                    path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("archive entry {} is absolute", path.display()));
            }
        }
    }
    match normalized.as_slice() {
        [] => Ok(None),
        [_] => Ok(Some(normalized.into_iter().collect())),
        _ => Err(format!(
            "archive entry {} must be a top-level file",
            path.display()
        )),
    }
}

fn validate_layout(
    stage: &Path,
    cli_name: &str,
    plugin_name: &str,
    source_name: &str,
) -> Result<(), String> {
    let actual = fs::read_dir(stage)
        .map_err(|error| format!("read release smoke stage: {error}"))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("read staged entry: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("read staged entry type: {error}"))?
                .is_file()
            {
                return Err("staged archive entries must be regular files".to_owned());
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| "staged archive entry name must be UTF-8".to_owned())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected = BTreeSet::from([
        cli_name.to_owned(),
        plugin_name.to_owned(),
        source_name.to_owned(),
    ]);
    if actual != expected {
        return Err(format!(
            "archive layout mismatch: expected {expected:?}, got {actual:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
