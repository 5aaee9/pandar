use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const MAX_PRODUCTION_MODULE_LINES: usize = 400;
const SOURCE_ROOTS: [&str; 3] = ["crates", "frontend", "mobile/android"];

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<std::ffi::OsString>) -> Result<(), String> {
    let Some(command) = args.first() else {
        return Err(usage());
    };
    if command.as_os_str() != std::ffi::OsStr::new("module-size") {
        return Err(usage());
    }
    let root = match args.get(1) {
        Some(root) => PathBuf::from(root.as_os_str()),
        None => workspace_root()?,
    };
    if args.len() > 2 {
        return Err(usage());
    }

    check_module_size(&root)
}

fn usage() -> String {
    "usage: pandar-quality module-size [workspace-root]".to_owned()
}

fn workspace_root() -> Result<PathBuf, String> {
    let current = env::current_dir().map_err(|error| format!("read current directory: {error}"))?;
    current
        .ancestors()
        .find(|path| {
            path.join("Cargo.toml").is_file()
                && path.join("crates").is_dir()
                && path.join("frontend").is_dir()
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not locate the Pandar workspace root".to_owned())
}

fn check_module_size(root: &Path) -> Result<(), String> {
    let mut oversized = Vec::new();
    for source_root in SOURCE_ROOTS {
        collect_oversized_modules(root, &root.join(source_root), &mut oversized)?;
    }
    oversized.sort();
    if oversized.is_empty() {
        println!("production modules are within {MAX_PRODUCTION_MODULE_LINES} lines");
        return Ok(());
    }

    Err(format!(
        "production modules exceed {MAX_PRODUCTION_MODULE_LINES} lines:\n{}",
        oversized.join("\n")
    ))
}

fn collect_oversized_modules(
    root: &Path,
    dir: &Path,
    oversized: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("read source directory {}: {error}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("read source entry in {}: {error}", dir.display()))?
            .path();
        if path.is_dir() {
            if !is_ignored_directory(&path) {
                collect_oversized_modules(root, &path, oversized)?;
            }
            continue;
        }
        if !is_production_module(&path) {
            continue;
        }

        let source = fs::read_to_string(&path)
            .map_err(|error| format!("read production source {}: {error}", path.display()))?;
        let line_count = source.lines().count();
        if line_count > MAX_PRODUCTION_MODULE_LINES {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            oversized.push(format!("{}: {line_count}", relative.display()));
        }
    }
    Ok(())
}

fn is_production_module(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str());
    matches!(
        extension,
        Some("rs" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "ts" | "tsx" | "kt")
    ) && !is_test_source(path)
}

fn is_ignored_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            "node_modules"
                | ".next"
                | ".gradle"
                | "build"
                | "dist"
                | "generated"
                | "out"
                | "target"
        )
    )
}

fn is_test_source(path: &Path) -> bool {
    if path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("tests" | "test" | "androidTest" | "testFixtures" | "__tests__")
        )
    }) {
        return true;
    }

    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == "tests.rs"
        || name.ends_with("_test.rs")
        || name.ends_with("_tests.rs")
        || name.contains(".test.")
        || name.contains(".spec.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_workspace_production_and_test_modules() {
        assert!(is_production_module(Path::new("src/module.c")));
        assert!(is_production_module(Path::new("src/main/kotlin/Module.kt")));
        assert!(!is_production_module(Path::new(
            "src/test/kotlin/ModuleTest.kt"
        )));
        assert!(!is_production_module(Path::new("app/module.test.tsx")));
        assert!(!is_production_module(Path::new(
            "src/external_race_tests.rs"
        )));
        assert!(is_ignored_directory(Path::new("build")));
        assert!(is_ignored_directory(Path::new("generated")));
    }

    #[test]
    fn reports_oversized_production_modules() {
        let root = env::temp_dir().join(format!("pandar-quality-{}", std::process::id()));
        for source_root in SOURCE_ROOTS {
            fs::create_dir_all(root.join(source_root)).unwrap();
        }
        fs::write(
            root.join("crates/oversized.rs"),
            "production line\n".repeat(MAX_PRODUCTION_MODULE_LINES + 1),
        )
        .unwrap();

        let error = check_module_size(&root).unwrap_err();
        assert!(error.contains("oversized.rs: 401"));
        fs::remove_dir_all(root).unwrap();
    }
}
