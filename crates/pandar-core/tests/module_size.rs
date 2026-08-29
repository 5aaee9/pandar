use std::{fs, path::Path};

const MAX_PRODUCTION_MODULE_LINES: usize = 400;

#[test]
fn workspace_production_modules_stay_under_line_limit() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("pandar-core should live under crates/");
    let mut oversized = Vec::new();
    collect_oversized_modules(&workspace.join("crates"), &mut oversized);
    collect_oversized_modules(&workspace.join("frontend"), &mut oversized);
    collect_oversized_modules(&workspace.join("mobile/android"), &mut oversized);

    assert!(
        oversized.is_empty(),
        "production modules exceed {MAX_PRODUCTION_MODULE_LINES} lines:\n{}",
        oversized.join("\n")
    );
}

fn collect_oversized_modules(dir: &Path, oversized: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("workspace source directory should be readable") {
        let entry = entry.expect("workspace source entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            if !is_ignored_directory(&path) {
                collect_oversized_modules(&path, oversized);
            }
            continue;
        }
        if !is_production_module(&path) {
            continue;
        }

        let source = fs::read_to_string(&path).expect("production source should be readable");
        let line_count = source.lines().count();
        if line_count > MAX_PRODUCTION_MODULE_LINES {
            oversized.push(format!("{}: {line_count}", path.display()));
        }
    }
}

fn is_production_module(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str());
    matches!(
        extension,
        Some("rs" | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "ts" | "tsx" | "kt")
    ) && !is_test_source(path)
}

#[test]
fn c_and_kotlin_sources_are_production_modules() {
    assert!(is_production_module(Path::new("module.c")));
    assert!(is_production_module(Path::new("src/main/kotlin/Module.kt")));
    assert!(!is_production_module(Path::new(
        "src/test/kotlin/ModuleTest.kt"
    )));
    assert!(!is_production_module(Path::new(
        "src/androidTest/kotlin/ModuleTest.kt"
    )));
    assert!(is_ignored_directory(Path::new("build")));
    assert!(is_ignored_directory(Path::new("generated")));
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
        || name.contains(".test.")
        || name.contains(".spec.")
}
