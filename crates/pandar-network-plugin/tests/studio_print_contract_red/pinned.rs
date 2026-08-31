use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const STUDIO_COMMIT: &str = "42d319c6692fa8e64790fddf0cdaafd2a4254bcc";
pub(super) const TASK_CONSUMER_HASH: &str = "e0a5f49f17863054eb6e782a6f707b20a5f6a28c";
pub(super) const SUBTASK_CONSUMER_HASH: &str = "230e2916b16a7b87b89a8f5ee57b539fdbd6ab3b";
pub(super) const MODEL_TASK_STATUS_HASH: &str = "a9156f1c3f9e2fd40ae7215aeb4d26b172021f4e";
pub(super) const MODEL_TASK_LAYOUT_HASH: &str = "afc8d530b4723eec0971824609ac6d3dae3bb58d";
pub(super) const MODEL_TASK_CALLBACK_HASH: &str = "15226032d4729ce90e5940d3cc41f70b60bd304d";
pub(super) const MODEL_TASK_FORWARDING_HASH: &str = "7790222a37d042f2704cfb0fd78f01021aec52fe";

const PRINT_HEADER: &str = "src/slic3r/Utils/bambu_networking.hpp";
const TASK_MANAGER: &str = "src/slic3r/GUI/TaskManager.cpp";
const TASK_MANAGER_BLOB: &str = "7a7bdcce2b747902db9dfa09aa68a2044d09dfdb";
const DEVICE_MANAGER: &str = "src/slic3r/GUI/DeviceManager.cpp";
const DEVICE_MANAGER_BLOB: &str = "559c64deda5d8cdb3bbdb6cf6ace352d716ae616";
const STATUS_PANEL: &str = "src/slic3r/GUI/StatusPanel.cpp";
const STATUS_PANEL_BLOB: &str = "60c62ff654ceccec4d323125fe1dbcae89c119cd";
const PROJECT_TASK_HEADER: &str = "src/libslic3r/ProjectTask.hpp";
const PROJECT_TASK_HEADER_BLOB: &str = "94605a43b9756353a91d0714057d32db470ac8cd";
const NETWORK_AGENT: &str = "src/slic3r/Utils/NetworkAgent.cpp";
const NETWORK_AGENT_BLOB: &str = "564943799bd407c689c0048f3d95b9c1c11ea095";
const NLOHMANN_TREE: &str = "src/nlohmann";
const NLOHMANN_TREE_BLOB: &str = "adcc3cb63df154808be3958507cfce00ee1933a5";

pub(super) fn stage(workspace: &Path, destination: &Path) {
    let studio = workspace.join("reference/BambuStudio");
    let series = pandar_studio_profile::abi_series(pandar_network_plugin::STUDIO_ABI_SERIES)
        .expect("selected Studio ABI series is catalogued");
    verify_commit(&studio, &series.studio_commit);
    verify_object(&studio, TASK_MANAGER, TASK_MANAGER_BLOB);
    verify_object(&studio, DEVICE_MANAGER, DEVICE_MANAGER_BLOB);
    verify_object(&studio, STATUS_PANEL, STATUS_PANEL_BLOB);
    verify_object(&studio, PROJECT_TASK_HEADER, PROJECT_TASK_HEADER_BLOB);
    verify_object(&studio, NETWORK_AGENT, NETWORK_AGENT_BLOB);
    verify_object(&studio, NLOHMANN_TREE, NLOHMANN_TREE_BLOB);

    fs::write(
        destination.join("bambu_networking.hpp"),
        show_at(&studio, &series.studio_commit, PRINT_HEADER),
    )
    .expect("stage pinned Bambu Studio print ABI header");
    stage_tree(&studio, NLOHMANN_TREE, destination);

    let task_source = show(&studio, TASK_MANAGER);
    let task_consumer = source_lines(&task_source, 321, 381);
    assert_eq!(hash_object(&task_consumer), TASK_CONSUMER_HASH);
    fs::write(
        destination.join("TaskManager.321-381.pinned.cpp"),
        &task_consumer,
    )
    .expect("stage pinned task-list consumer excerpt");

    let device_source = show(&studio, DEVICE_MANAGER);
    let subtask_consumer = source_lines(&device_source, 3877, 3976);
    assert_eq!(hash_object(&subtask_consumer), SUBTASK_CONSUMER_HASH);
    fs::write(
        destination.join("DeviceManager.3877-3976.pinned.cpp"),
        &subtask_consumer,
    )
    .expect("stage pinned subtask consumer excerpt");

    let status_source = show(&studio, STATUS_PANEL);
    let model_task_status = source_lines(&status_source, 4143, 4160);
    assert_eq!(hash_object(&model_task_status), MODEL_TASK_STATUS_HASH);
    fs::write(
        destination.join("StatusPanel.4143-4160.pinned.cpp"),
        &model_task_status,
    )
    .expect("stage pinned model-task caller excerpt");

    let project_task_source = show(&studio, PROJECT_TASK_HEADER);
    let model_task_layout = source_lines(&project_task_source, 153, 166);
    assert_eq!(hash_object(&model_task_layout), MODEL_TASK_LAYOUT_HASH);
    let model_task_callback = source_lines(&project_task_source, 251, 251);
    assert_eq!(hash_object(&model_task_callback), MODEL_TASK_CALLBACK_HASH);
    let mut model_task_header =
        b"#pragma once\n#include <functional>\n#include <string>\nnamespace Slic3r {\n".to_vec();
    model_task_header.extend_from_slice(&model_task_layout);
    model_task_header.extend_from_slice(&model_task_callback);
    model_task_header.extend_from_slice(b"}\n");
    fs::write(destination.join("pinned_model_task.hpp"), model_task_header)
        .expect("stage pinned model-task ABI header");

    let network_source = show(&studio, NETWORK_AGENT);
    let model_task_forwarding = source_lines(&network_source, 1564, 1573);
    assert_eq!(
        hash_object(&model_task_forwarding),
        MODEL_TASK_FORWARDING_HASH
    );
    fs::write(
        destination.join("NetworkAgent.1564-1573.pinned.cpp"),
        &model_task_forwarding,
    )
    .expect("stage pinned model-task forwarding excerpt");

    fs::write(
        destination.join("pinned_consumer_hashes.hpp"),
        format!(
            "#pragma once\n#define PANDAR_TASK_CONSUMER_HASH \"{TASK_CONSUMER_HASH}\"\n#define PANDAR_SUBTASK_CONSUMER_HASH \"{SUBTASK_CONSUMER_HASH}\"\n#define PANDAR_MODEL_TASK_STATUS_HASH \"{MODEL_TASK_STATUS_HASH}\"\n#define PANDAR_MODEL_TASK_LAYOUT_HASH \"{MODEL_TASK_LAYOUT_HASH}\"\n#define PANDAR_MODEL_TASK_CALLBACK_HASH \"{MODEL_TASK_CALLBACK_HASH}\"\n#define PANDAR_MODEL_TASK_FORWARDING_HASH \"{MODEL_TASK_FORWARDING_HASH}\"\n"
        ),
    )
    .expect("stage pinned consumer hash header");
}

fn verify_commit(studio: &Path, commit: &str) {
    let object = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["rev-parse", &format!("{commit}^{{commit}}")])
        .output()
        .expect("resolve selected Studio commit");
    assert!(
        object.status.success(),
        "resolve selected Studio commit {commit}: {}",
        String::from_utf8_lossy(&object.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&object.stdout).trim(), commit);
}

fn verify_object(studio: &Path, path: &str, expected: &str) {
    let object = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["rev-parse", &format!("{STUDIO_COMMIT}:{path}")])
        .output()
        .expect("resolve pinned Studio object");
    assert!(
        object.status.success(),
        "resolve pinned Studio object {path}: {}",
        String::from_utf8_lossy(&object.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&object.stdout).trim(), expected);
}

fn show(studio: &Path, path: &str) -> Vec<u8> {
    show_at(studio, STUDIO_COMMIT, path)
}

fn show_at(studio: &Path, commit: &str, path: &str) -> Vec<u8> {
    let object = format!("{commit}:{path}");
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["show", &object])
        .output()
        .expect("read pinned Studio source");
    assert!(
        output.status.success(),
        "read pinned Studio source {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn stage_tree(studio: &Path, root: &str, destination: &Path) {
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["ls-tree", "-r", "--name-only", STUDIO_COMMIT, "--", root])
        .output()
        .expect("list pinned nlohmann headers");
    assert!(output.status.success(), "list pinned nlohmann headers");
    for path in String::from_utf8(output.stdout).unwrap().lines() {
        let relative = PathBuf::from(path)
            .strip_prefix("src")
            .expect("nlohmann path is below src")
            .to_owned();
        let target = destination.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, show(studio, path)).unwrap();
    }
}

fn source_lines(source: &[u8], first: usize, last: usize) -> Vec<u8> {
    let source = String::from_utf8(source.to_vec()).expect("pinned source is UTF-8");
    let mut excerpt = source
        .lines()
        .skip(first - 1)
        .take(last - first + 1)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    excerpt.push(b'\n');
    excerpt
}

fn hash_object(bytes: &[u8]) -> String {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("launch git hash-object");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child
        .wait_with_output()
        .expect("hash pinned source excerpt");
    assert!(output.status.success(), "hash pinned source excerpt");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
