use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use crate::source::{PINNED_BOOST_SHA256, PINNED_BOOST_VERSION, PINNED_BOOST_VERSION_NUMBER};

pub struct BoostArchive {
    _extracted: tempfile::TempDir,
    pub include_roots: Vec<PathBuf>,
    pub sha256: String,
}

pub fn prepare_archive(path: &Path) -> Result<BoostArchive, String> {
    let extracted = tempfile::tempdir()
        .map_err(|error| format!("create Boost extraction directory: {error}"))?;
    let owned_archive = extracted.path().join("boost-1.84.0.tar.gz");
    let sha256 = copy_and_hash(path, &owned_archive)?;
    verify_pinned_digest(&sha256)?;
    let unpacked = extracted.path().join("unpacked");
    fs::create_dir(&unpacked).map_err(|error| {
        format!(
            "create Boost unpack directory {}: {error}",
            unpacked.display()
        )
    })?;
    unpack_archive(&owned_archive, &unpacked)?;
    let include_roots = locate_include_roots(&unpacked)?;
    let version_roots = include_roots
        .iter()
        .filter(|include| include.join("boost/version.hpp").is_file())
        .collect::<Vec<_>>();
    let [version_root] = version_roots.as_slice() else {
        return Err(format!(
            "Boost archive must contain exactly one boost/version.hpp include root, got {}",
            version_roots.len()
        ));
    };
    verify_version(version_root)?;
    Ok(BoostArchive {
        _extracted: extracted,
        include_roots,
        sha256,
    })
}

fn copy_and_hash(source: &Path, destination: &Path) -> Result<String, String> {
    let source_file = File::open(source)
        .map_err(|error| format!("open Boost archive {}: {error}", source.display()))?;
    let destination_file = File::create(destination).map_err(|error| {
        format!(
            "create owned Boost archive copy {}: {error}",
            destination.display()
        )
    })?;
    let mut reader = BufReader::new(source_file);
    let mut writer = BufWriter::new(destination_file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("read Boost archive {}: {error}", source.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        writer.write_all(&buffer[..count]).map_err(|error| {
            format!(
                "write owned Boost archive copy {}: {error}",
                destination.display()
            )
        })?;
    }
    writer.flush().map_err(|error| {
        format!(
            "flush owned Boost archive copy {}: {error}",
            destination.display()
        )
    })?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_pinned_digest(actual: &str) -> Result<(), String> {
    if actual == PINNED_BOOST_SHA256 {
        return Ok(());
    }
    Err(format!(
        "Boost archive SHA-256 mismatch: expected {PINNED_BOOST_SHA256}, got {actual}"
    ))
}

fn unpack_archive(path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(path)
        .map_err(|error| format!("open Boost archive {}: {error}", path.display()))?;
    let decoder = GzDecoder::new(BufReader::new(file));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| format!("read Boost archive entries {}: {error}", path.display()))?;
    for entry in entries {
        let mut entry = entry
            .map_err(|error| format!("read Boost archive entry {}: {error}", path.display()))?;
        let entry_path = entry
            .path()
            .map_err(|error| format!("read Boost archive entry path {}: {error}", path.display()))?
            .into_owned();
        let unpacked = entry.unpack_in(destination).map_err(|error| {
            format!(
                "unpack Boost archive entry {:?} from {}: {error}",
                entry_path,
                path.display()
            )
        })?;
        if !unpacked {
            return Err(format!(
                "refuse Boost archive entry outside extraction directory: {:?}",
                entry_path
            ));
        }
    }
    Ok(())
}

fn locate_include_roots(extracted: &Path) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    if extracted.join("boost/version.hpp").is_file() {
        candidates.push(extracted.to_path_buf());
    }
    let entries = fs::read_dir(extracted).map_err(|error| {
        format!(
            "inspect extracted Boost archive {}: {error}",
            extracted.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "inspect extracted Boost archive {}: {error}",
                extracted.display()
            )
        })?;
        let path = entry.path();
        if path.join("boost/version.hpp").is_file() {
            candidates.push(path);
        }
    }
    if candidates.len() > 1 {
        return Err(format!(
            "Boost archive contains multiple boost/version.hpp include roots under {}",
            extracted.display()
        ));
    }
    if !candidates.is_empty() {
        return Ok(candidates);
    }

    let mut module_roots = Vec::new();
    collect_metadata_include_roots(&extracted.join("libs"), &mut module_roots)?;
    let entries = fs::read_dir(extracted).map_err(|error| {
        format!(
            "inspect extracted Boost archive {}: {error}",
            extracted.display()
        )
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| {
                format!(
                    "inspect extracted Boost archive {}: {error}",
                    extracted.display()
                )
            })?
            .path();
        collect_metadata_include_roots(&path.join("libs"), &mut module_roots)?;
    }
    module_roots.sort();
    module_roots.dedup();
    if module_roots.is_empty() {
        return Err(format!(
            "Boost archive does not contain an aggregate or modular include tree under {}",
            extracted.display()
        ));
    }
    Ok(module_roots)
}

fn collect_metadata_include_roots(path: &Path, roots: &mut Vec<PathBuf>) -> Result<(), String> {
    if !path.is_dir() {
        return Ok(());
    }
    let include = path.join("include");
    if path.join("meta/libraries.json").is_file() && include.join("boost").is_dir() {
        roots.push(include);
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("inspect Boost module directory {}: {error}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("inspect Boost module directory {}: {error}", path.display())
        })?;
        let file_name = entry.file_name();
        if !matches!(file_name.to_str(), Some("include" | "meta"))
            && entry
                .file_type()
                .map_err(|error| {
                    format!(
                        "inspect Boost module entry {}: {error}",
                        entry.path().display()
                    )
                })?
                .is_dir()
        {
            collect_metadata_include_roots(&entry.path(), roots)?;
        }
    }
    Ok(())
}

fn verify_version(include_root: &Path) -> Result<(), String> {
    let version_path = include_root.join("boost/version.hpp");
    let version = fs::read_to_string(&version_path).map_err(|error| {
        format!(
            "read pinned Boost header {}: {error}",
            version_path.display()
        )
    })?;
    let expected = format!("#define BOOST_VERSION {PINNED_BOOST_VERSION_NUMBER}");
    if !version.lines().any(|line| line.trim() == expected) {
        return Err(format!(
            "Boost archive must contain pinned version {PINNED_BOOST_VERSION}: {}",
            include_root.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{locate_include_roots, prepare_archive, sha256_bytes, verify_pinned_digest};

    #[test]
    fn hashes_known_bytes_with_sha256() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rejects_archive_digest_mismatch() {
        let actual = sha256_bytes(b"not the pinned Boost archive");

        let error = verify_pinned_digest(&actual).unwrap_err();

        assert!(error.contains(&actual), "unexpected error: {error}");
        assert!(
            error.contains("4d27e9efed0f6f152dc28db6430b9d3dfb40c0345da7342eaa5a987dde57bd95"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_mismatched_archive_before_unpacking() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("boost-1.84.0.tar.gz");
        fs::write(&archive, b"not a gzip archive").unwrap();

        let error = prepare_archive(&archive).err().unwrap();

        assert!(
            error.contains("SHA-256 mismatch"),
            "unexpected error: {error}"
        );
        assert!(!error.contains("gzip"), "unexpected error: {error}");
    }

    #[test]
    fn locates_version_header_under_archive_top_level_directory() {
        let temp = tempfile::tempdir().unwrap();
        let include = temp.path().join("boost-1.84.0");
        fs::create_dir_all(include.join("boost")).unwrap();
        fs::write(
            include.join("boost/version.hpp"),
            "#define BOOST_VERSION 108400\n",
        )
        .unwrap();

        assert_eq!(locate_include_roots(temp.path()).unwrap(), vec![include]);
    }

    #[test]
    fn locates_all_headers_in_official_modular_release_layout() {
        let temp = tempfile::tempdir().unwrap();
        let config_include = temp.path().join("boost-1.84.0/libs/config/include");
        let function_include = temp.path().join("boost-1.84.0/libs/function/include");
        let numeric_include = temp
            .path()
            .join("boost-1.84.0/libs/numeric/conversion/include");
        let test_include = temp
            .path()
            .join("boost-1.84.0/libs/beast/test/extras/include");
        fs::create_dir_all(config_include.join("boost")).unwrap();
        fs::create_dir_all(function_include.join("boost")).unwrap();
        fs::create_dir_all(numeric_include.join("boost")).unwrap();
        fs::create_dir_all(test_include.join("boost")).unwrap();
        for metadata in [
            temp.path().join("boost-1.84.0/libs/config/meta"),
            temp.path().join("boost-1.84.0/libs/function/meta"),
            temp.path()
                .join("boost-1.84.0/libs/numeric/conversion/meta"),
        ] {
            fs::create_dir_all(&metadata).unwrap();
            fs::write(metadata.join("libraries.json"), "[]\n").unwrap();
        }
        fs::write(
            config_include.join("boost/version.hpp"),
            "#define BOOST_VERSION 108400\n",
        )
        .unwrap();
        fs::write(
            function_include.join("boost/function.hpp"),
            "// modular function header\n",
        )
        .unwrap();
        fs::write(
            numeric_include.join("boost/numeric.hpp"),
            "// nested numeric header\n",
        )
        .unwrap();
        fs::write(
            test_include.join("boost/beast_test.hpp"),
            "// test-only header\n",
        )
        .unwrap();

        let mut expected = vec![config_include, function_include, numeric_include];
        expected.sort();
        assert_eq!(locate_include_roots(temp.path()).unwrap(), expected);
    }
}
