use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::{
    source_mapping::{ExportMap, loaded_export_map},
    types::cpp_struct_fields,
};

pub const PINNED_STUDIO_COMMIT: &str = "42d319c6692fa8e64790fddf0cdaafd2a4254bcc";
pub const PINNED_BOOST_VERSION: &str = "1.84.0";
pub const PINNED_BOOST_VERSION_NUMBER: &str = "108400";
pub const PINNED_BOOST_SHA256: &str =
    "4d27e9efed0f6f152dc28db6430b9d3dfb40c0345da7342eaa5a987dde57bd95";

const CONTRACT_PATHS: &[&str] = &[
    "version.inc",
    "deps/Boost/Boost.cmake",
    "src/slic3r/Utils/bambu_networking.hpp",
    "src/slic3r/Utils/NetworkAgent.cpp",
    "src/slic3r/Utils/NetworkAgent.hpp",
    "src/slic3r/Utils/FileTransferUtils.cpp",
    "src/slic3r/Utils/FileTransferUtils.hpp",
    "src/slic3r/GUI/GUI_App.cpp",
    "src/libslic3r/ProjectTask.hpp",
];

#[derive(Debug)]
pub struct StudioContract {
    pub commit: String,
    pub studio_version: String,
    pub network_agent_version: String,
    pub network_symbols: BTreeSet<String>,
    pub file_transfer_symbols: BTreeSet<String>,
    pub network_exports: ExportMap,
    pub file_transfer_exports: ExportMap,
    pub print_params_fields: Vec<String>,
}

pub fn inspect_source(root: &Path, expected_commit: &str) -> Result<StudioContract, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("resolve Studio source {}: {error}", root.display()))?;
    let origin = git(&root, &["remote", "get-url", "origin"])?;
    if !is_official_origin(&origin) {
        return Err(format!(
            "Studio origin must be the official bambulab/BambuStudio repository, got {origin:?}"
        ));
    }

    let commit = git(&root, &["rev-parse", "HEAD"])?;
    if commit != expected_commit {
        return Err(format!(
            "Studio HEAD must be pinned commit {expected_commit}, got {commit}"
        ));
    }

    let mut status_args = vec!["status", "--porcelain=v1", "--untracked-files=no", "--"];
    status_args.extend_from_slice(CONTRACT_PATHS);
    let drift = git(&root, &status_args)?;
    if !drift.is_empty() {
        return Err(format!(
            "Studio tracked contract files differ from pinned HEAD:\n{drift}"
        ));
    }

    let version = read_contract_file(&root, "version.inc")?;
    let boost = read_contract_file(&root, "deps/Boost/Boost.cmake")?;
    let networking = read_contract_file(&root, "src/slic3r/Utils/bambu_networking.hpp")?;
    let network_agent = read_contract_file(&root, "src/slic3r/Utils/NetworkAgent.cpp")?;
    let file_transfer = read_contract_file(&root, "src/slic3r/Utils/FileTransferUtils.cpp")?;
    let gui_app = read_contract_file(&root, "src/slic3r/GUI/GUI_App.cpp")?;
    let project_task = read_contract_file(&root, "src/libslic3r/ProjectTask.hpp")?;

    let studio_version = quoted_value(&version, "set(SLIC3R_VERSION ")?;
    let network_agent_version = quoted_value(&networking, "#define BAMBU_NETWORK_AGENT_VERSION")?;
    let print_params_fields = cpp_struct_fields(&networking, "PrintParams")?;
    let network_exports = loaded_export_map(
        &network_agent,
        "get_network_function",
        "reinterpret_cast<",
        "bambu_network_",
    )?;
    let file_transfer_exports =
        loaded_export_map(&file_transfer, "sym_lookup", "sym_lookup<", "ft_")?;
    let network_symbols = export_symbols(&network_exports);
    let file_transfer_symbols = export_symbols(&file_transfer_exports);
    if !boost.contains(&format!("boost-{PINNED_BOOST_VERSION}.tar.gz"))
        || !boost.contains(&format!("SHA256={PINNED_BOOST_SHA256}"))
    {
        return Err("pinned Studio Boost URL or SHA-256 dependency contract drifted".to_owned());
    }
    if network_symbols.is_empty() {
        return Err("NetworkAgent.cpp contains no loaded bambu_network_* symbols".to_owned());
    }
    if file_transfer_symbols.is_empty() {
        return Err("FileTransferUtils.cpp contains no loaded ft_* symbols".to_owned());
    }
    let compact_gui = gui_app.split_whitespace().collect::<String>();
    if !compact_gui.contains("network_ver.substr(0,8)==studio_ver.substr(0,8)") {
        return Err(
            "GUI_App.cpp no longer uses the reviewed first-eight-character network version gate"
                .to_owned(),
        );
    }
    let compact_project_task = project_task
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !compact_project_task.contains("class BBLModelTask;")
        || !compact_project_task
            .contains("typedef std::function<void(BBLModelTask* subtask)> OnGetSubTaskFn;")
    {
        return Err(
            "ProjectTask.hpp no longer contains the reviewed BBLModelTask/OnGetSubTaskFn ABI dependency"
                .to_owned(),
        );
    }
    if expected_commit == PINNED_STUDIO_COMMIT
        && (network_symbols.len() != 108 || file_transfer_symbols.len() != 21)
    {
        return Err(format!(
            "pinned Studio symbol extraction drifted: expected 108 network and 21 FT, got {} network and {} FT",
            network_symbols.len(),
            file_transfer_symbols.len()
        ));
    }

    Ok(StudioContract {
        commit,
        studio_version,
        network_agent_version,
        network_symbols,
        file_transfer_symbols,
        network_exports,
        file_transfer_exports,
        print_params_fields,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("run git {args:?} in {}: {error}", root.display()))?;
    command_stdout(root, args, output)
}

fn command_stdout(root: &Path, args: &[&str], output: Output) -> Result<String, String> {
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed in {} with {}: {}",
            root.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.trim().to_owned())
        .map_err(|error| format!("git {args:?} returned non-UTF-8 output: {error}"))
}

fn is_official_origin(origin: &str) -> bool {
    let normalized = origin.trim().trim_end_matches('/').trim_end_matches(".git");
    matches!(
        normalized,
        "https://github.com/bambulab/BambuStudio"
            | "git@github.com:bambulab/BambuStudio"
            | "ssh://git@github.com/bambulab/BambuStudio"
    )
}

fn read_contract_file(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|error| format!("read Studio contract file {}: {error}", path.display()))
}

fn quoted_value(contents: &str, marker: &str) -> Result<String, String> {
    let line = contents
        .lines()
        .find(|line| line.trim_start().starts_with(marker))
        .ok_or_else(|| format!("missing {marker:?} in Studio contract source"))?;
    let start = line
        .find('"')
        .ok_or_else(|| format!("missing quoted value after {marker:?}"))?
        + 1;
    let value = line[start..]
        .split('"')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing quoted value after {marker:?}"))?;
    Ok(value.to_owned())
}

fn export_symbols(exports: &ExportMap) -> BTreeSet<String> {
    exports.iter().map(|(symbol, _)| symbol.clone()).collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use super::{
        CONTRACT_PATHS, PINNED_BOOST_VERSION, PINNED_BOOST_VERSION_NUMBER, PINNED_STUDIO_COMMIT,
        inspect_source,
    };

    const OFFICIAL_ORIGIN: &str = "https://github.com/bambulab/BambuStudio";

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Pandar Contract Test")
            .env("GIT_AUTHOR_EMAIL", "contract@example.invalid")
            .env("GIT_COMMITTER_NAME", "Pandar Contract Test")
            .env("GIT_COMMITTER_EMAIL", "contract@example.invalid")
            .output()
            .expect("run git for contract fixture");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn fixture() -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("create source fixture");
        for path in CONTRACT_PATHS {
            let path = temp.path().join(path);
            fs::create_dir_all(path.parent().expect("contract path has parent"))
                .expect("create contract fixture parent");
            let contents = match path.strip_prefix(temp.path()).unwrap().to_str().unwrap() {
                "version.inc" => "set(SLIC3R_VERSION \"02.07.01.62\")\n",
                "deps/Boost/Boost.cmake" => {
                    "URL \"https://example.invalid/boost-1.84.0.tar.gz\"\nURL_HASH SHA256=4d27e9efed0f6f152dc28db6430b9d3dfb40c0345da7342eaa5a987dde57bd95\n"
                }
                "src/slic3r/Utils/bambu_networking.hpp" => {
                    "#define BAMBU_NETWORK_AGENT_VERSION \"02.07.01.51\"\nstruct PrintParams { std::string dev_id; };\n"
                }
                "src/slic3r/Utils/NetworkAgent.cpp" => {
                    "reinterpret_cast<func_get_version>(get_network_function(\"bambu_network_get_version\"));\nreinterpret_cast<func_start>(get_network_function(\"bambu_network_start\"));\n"
                }
                "src/slic3r/Utils/FileTransferUtils.cpp" => {
                    "sym_lookup<fn_ft_abi_version>(networking_, \"ft_abi_version\");\nsym_lookup<fn_ft_free>(networking_, \"ft_free\");\n"
                }
                "src/slic3r/GUI/GUI_App.cpp" => {
                    "network_ver.substr(0, 8) == studio_ver.substr(0, 8);\n"
                }
                "src/libslic3r/ProjectTask.hpp" => {
                    "class BBLModelTask;\ntypedef std::function<void(BBLModelTask* subtask)> OnGetSubTaskFn;\n"
                }
                _ => "// real upstream declaration surface\n",
            };
            fs::write(path, contents).expect("write contract fixture");
        }
        git(temp.path(), &["init"]);
        git(temp.path(), &["remote", "add", "origin", OFFICIAL_ORIGIN]);
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-m", "contract fixture"]);
        temp
    }

    #[test]
    fn pinned_commit_is_the_reviewed_upstream_object() {
        assert_eq!(
            PINNED_STUDIO_COMMIT,
            "42d319c6692fa8e64790fddf0cdaafd2a4254bcc"
        );
        assert_eq!(PINNED_BOOST_VERSION, "1.84.0");
        assert_eq!(PINNED_BOOST_VERSION_NUMBER, "108400");
    }

    #[test]
    fn extracts_versions_and_loaded_symbols_from_official_clean_checkout() {
        let temp = fixture();
        let head = git(temp.path(), &["rev-parse", "HEAD"]);

        let contract = inspect_source(temp.path(), &head).expect("inspect valid official checkout");

        assert_eq!(contract.commit, head);
        assert_eq!(contract.studio_version, "02.07.01.62");
        assert_eq!(contract.network_agent_version, "02.07.01.51");
        assert_eq!(contract.print_params_fields, ["dev_id"]);
        assert_eq!(
            contract.network_exports,
            [
                (
                    "bambu_network_get_version".to_owned(),
                    "func_get_version".to_owned()
                ),
                ("bambu_network_start".to_owned(), "func_start".to_owned())
            ]
        );
        assert_eq!(
            contract.network_symbols.into_iter().collect::<Vec<_>>(),
            ["bambu_network_get_version", "bambu_network_start"]
        );
        assert_eq!(
            contract
                .file_transfer_symbols
                .into_iter()
                .collect::<Vec<_>>(),
            ["ft_abi_version", "ft_free"]
        );
    }

    #[test]
    fn rejects_wrong_commit() {
        let temp = fixture();
        let error = inspect_source(temp.path(), PINNED_STUDIO_COMMIT).unwrap_err();

        assert!(error.contains("HEAD"), "unexpected error: {error}");
        assert!(
            error.contains(PINNED_STUDIO_COMMIT),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_non_official_origin() {
        let temp = fixture();
        let head = git(temp.path(), &["rev-parse", "HEAD"]);
        git(
            temp.path(),
            &[
                "remote",
                "set-url",
                "origin",
                "https://example.invalid/fork",
            ],
        );

        let error = inspect_source(temp.path(), &head).unwrap_err();

        assert!(error.contains("origin"), "unexpected error: {error}");
        assert!(
            error.contains("bambulab/BambuStudio"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_tracked_contract_drift_but_ignores_untracked_output() {
        let temp = fixture();
        let head = git(temp.path(), &["rev-parse", "HEAD"]);
        fs::write(temp.path().join("untracked-build.log"), "ignored\n").unwrap();
        inspect_source(temp.path(), &head).expect("ignore unrelated untracked output");

        fs::write(
            temp.path().join("src/slic3r/Utils/NetworkAgent.cpp"),
            "get_network_function(\"bambu_network_tampered\");\n",
        )
        .unwrap();
        let error = inspect_source(temp.path(), &head).unwrap_err();

        assert!(
            error.contains("tracked contract files"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("NetworkAgent.cpp"),
            "unexpected error: {error}"
        );
    }
}
