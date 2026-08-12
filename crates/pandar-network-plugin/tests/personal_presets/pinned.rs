use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn stage(series: &pandar_studio_profile::StudioAbiSeries, root: &Path) {
    let studio = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("network plugin is in workspace crates")
        .join("reference/BambuStudio");
    assert!(
        studio.join(".git").exists(),
        "pinned BambuStudio repository is required"
    );
    let pinned = Command::new("git")
        .arg("-C")
        .arg(&studio)
        .args(["rev-parse", &format!("{}^{{commit}}", series.studio_commit)])
        .output()
        .expect("resolve pinned Studio commit");
    assert!(
        pinned.status.success(),
        "pinned Studio commit {} is unavailable: {}",
        series.studio_commit,
        String::from_utf8_lossy(&pinned.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&pinned.stdout).trim(),
        series.studio_commit.as_str()
    );
    for path in [
        "src/slic3r/Utils/NetworkAgent.hpp",
        "src/slic3r/Utils/bambu_networking.hpp",
    ] {
        stage_header(&studio, &series.studio_commit, root, path);
    }
    // Preset aliases only require these ProjectTask forward declarations.
    let project_task = root.join("libslic3r/ProjectTask.hpp");
    fs::create_dir_all(project_task.parent().unwrap()).unwrap();
    let project_task_stub = [
        "#pragma once",
        "#include <functional>",
        "namespace Slic3r {",
        "class BBLModelTask;",
        "using OnGetSubTaskFn = std::function<void(BBLModelTask*)>;",
        "}",
        "",
    ]
    .join("\n");
    fs::write(project_task, project_task_stub).unwrap();
}

fn stage_header(studio: &Path, commit: &str, root: &Path, path: &str) {
    let target = root.join(path.strip_prefix("src/").unwrap());
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, git_show(studio, commit, path)).unwrap();
}

fn git_show(studio: &Path, commit: &str, path: &str) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["show", &format!("{commit}:{path}")])
        .output()
        .expect("read pinned Studio source");
    assert!(
        output.status.success(),
        "read pinned Studio source {commit}:{path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
