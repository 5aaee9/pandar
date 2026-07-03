use std::{fs, path::Path};

const MAX_PRODUCTION_MODULE_LINES: usize = 400;

#[test]
fn workspace_production_rust_modules_stay_under_line_limit() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("pandar-core should live under crates/");
    let mut oversized = Vec::new();
    collect_oversized_modules(&workspace.join("crates"), &mut oversized);

    assert!(
        oversized.is_empty(),
        "production Rust modules exceed {MAX_PRODUCTION_MODULE_LINES} lines:\n{}",
        oversized.join("\n")
    );
}

fn collect_oversized_modules(dir: &Path, oversized: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("workspace crates should be readable") {
        let entry = entry.expect("workspace crate entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_oversized_modules(&path, oversized);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            || is_test_source(&path)
        {
            continue;
        }

        let source = fs::read_to_string(&path).expect("Rust source should be readable");
        let line_count = source.lines().count();
        if line_count > MAX_PRODUCTION_MODULE_LINES {
            oversized.push(format!("{}: {line_count}", path.display()));
        }
    }
}

fn is_test_source(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "tests")
        || path.file_name().and_then(|name| name.to_str()) == Some("tests.rs")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_test.rs"))
}
