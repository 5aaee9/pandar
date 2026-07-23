use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) const STUDIO_COMMIT: &str = "ba049f6a2e08c3b6033660bb84da80c08722974b";

const DEVICE_MANAGER: &str = "src/slic3r/GUI/DeviceManager.cpp";
const DEVICE_MANAGER_BLOB: &str = "832e77d8bf5507fb97a96baa27c6c2ae5edc3e86";
const DEV_CONFIG: &str = "src/slic3r/GUI/DeviceCore/DevConfig.cpp";
const DEV_CONFIG_BLOB: &str = "9833731040aebe78a06259c834c0142deeda0660";
const DEV_AXIS: &str = "src/slic3r/GUI/DeviceCore/DevAxis.cpp";
const DEV_AXIS_BLOB: &str = "201e3afb00308ee6fa5b71f9b0553510670c7da7";
const DEV_PRINT_OPTIONS: &str = "src/slic3r/GUI/DeviceCore/DevPrintOptions.cpp";
const DEV_PRINT_OPTIONS_BLOB: &str = "8842e2d578fd5bb0a4ec9eb472d8f1db5fdfea71";
const DEV_STORAGE: &str = "src/slic3r/GUI/DeviceCore/DevStorage.cpp";
const DEV_STORAGE_BLOB: &str = "e5093471ed421e50b00cc80e735f05e634703dd5";
const DEV_INFO: &str = "src/slic3r/GUI/DeviceCore/DevInfo.cpp";
const DEV_INFO_BLOB: &str = "7ba0796a1b69398ae45808464bb503de22bd31a7";
const NLOHMANN_TREE: &str = "src/nlohmann";
const NLOHMANN_TREE_BLOB: &str = "adcc3cb63df154808be3958507cfce00ee1933a5";

struct SourceSlice {
    output: &'static str,
    source: &'static str,
    first: usize,
    last: usize,
    hash: &'static str,
}

const SLICES: &[SourceSlice] = &[
    SourceSlice {
        output: "device_check_enable.4280-4288.cpp",
        source: DEVICE_MANAGER,
        first: 4280,
        last: 4288,
        hash: "005b01e7c3ab50a55bae9ad36038fefd4306b91b",
    },
    SourceSlice {
        output: "device_cfg_camera.4367-4368.cpp",
        source: DEVICE_MANAGER,
        first: 4367,
        last: 4368,
        hash: "a98633d61e62439f431b293ba2185ce683758f93",
    },
    SourceSlice {
        output: "device_fun_agora.4376-4377.cpp",
        source: DEVICE_MANAGER,
        first: 4376,
        last: 4377,
        hash: "a067aa27942e03e0203682b6450dceb652dc091e",
    },
    SourceSlice {
        output: "device_fun_camera.4388-4390.cpp",
        source: DEVICE_MANAGER,
        first: 4388,
        last: 4390,
        hash: "8cdf6188237d156e2988428eb1e8fa1c8e788617",
    },
    SourceSlice {
        output: "device_fun_ext_change_assist.4393.cpp",
        source: DEVICE_MANAGER,
        first: 4393,
        last: 4393,
        hash: "051dbccc25601b98fd17597025c5032b9736f781",
    },
    SourceSlice {
        output: "device_aux.4433-4441.cpp",
        source: DEVICE_MANAGER,
        first: 4433,
        last: 4441,
        hash: "69c4c849a6b0e2a23e7e7785fcae9660aa280447",
    },
    SourceSlice {
        output: "device_fun_wtm.4396.cpp",
        source: DEVICE_MANAGER,
        first: 4396,
        last: 4396,
        hash: "92a490efaedd70e7d13cd158e9d62c4e4e187710",
    },
    SourceSlice {
        output: "device_flag_bits.4474-4485.cpp",
        source: DEVICE_MANAGER,
        first: 4474,
        last: 4485,
        hash: "6a26fe6c3f3bb13d283842b0080c51b91cc5c3b3",
    },
    SourceSlice {
        output: "dev_config.11-67.cpp",
        source: DEV_CONFIG,
        first: 11,
        last: 67,
        hash: "151ca25edd7a2d8e151284244ae8eaa67340f265",
    },
    SourceSlice {
        output: "dev_axis.9-18.cpp",
        source: DEV_AXIS,
        first: 9,
        last: 18,
        hash: "2930a7a651a3bce4b5759c04213ffe6ec417e646",
    },
    SourceSlice {
        output: "dev_options_cfg.227-231.cpp",
        source: DEV_PRINT_OPTIONS,
        first: 227,
        last: 231,
        hash: "7adad8d6bbebc88cbc989622d505886152171c6e",
    },
    SourceSlice {
        output: "dev_options_fun.234-247.cpp",
        source: DEV_PRINT_OPTIONS,
        first: 234,
        last: 247,
        hash: "8685abb3d407dd9abe610f8be8ac402ddd32887c",
    },
    SourceSlice {
        output: "dev_storage.7-16.cpp",
        source: DEV_STORAGE,
        first: 7,
        last: 16,
        hash: "5b5253b67fa7cdf42390328aecbb4ef740ba1d88",
    },
    SourceSlice {
        output: "dev_info.29-36.cpp",
        source: DEV_INFO,
        first: 29,
        last: 36,
        hash: "cbd2cbd24de81213d17bb9889e2beb3767022e1e",
    },
];

pub(super) fn stage(workspace: &Path, destination: &Path) {
    let studio = workspace.join("reference/BambuStudio");
    for (path, hash) in [
        (DEVICE_MANAGER, DEVICE_MANAGER_BLOB),
        (DEV_CONFIG, DEV_CONFIG_BLOB),
        (DEV_AXIS, DEV_AXIS_BLOB),
        (DEV_PRINT_OPTIONS, DEV_PRINT_OPTIONS_BLOB),
        (DEV_STORAGE, DEV_STORAGE_BLOB),
        (DEV_INFO, DEV_INFO_BLOB),
        (NLOHMANN_TREE, NLOHMANN_TREE_BLOB),
    ] {
        verify_object(&studio, path, hash);
    }
    stage_tree(&studio, NLOHMANN_TREE, destination);
    for slice in SLICES {
        let source = show(&studio, slice.source);
        let excerpt = source_lines(&source, slice.first, slice.last);
        assert_eq!(
            hash_object(&excerpt),
            slice.hash,
            "{} drifted",
            slice.output
        );
        fs::write(destination.join(slice.output), excerpt).unwrap();
    }
}

fn verify_object(studio: &Path, path: &str, expected: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["rev-parse", &format!("{STUDIO_COMMIT}:{path}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "resolve pinned Studio object {path}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
}

fn show(studio: &Path, path: &str) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["show", &format!("{STUDIO_COMMIT}:{path}")])
        .output()
        .unwrap();
    assert!(output.status.success(), "read pinned Studio source {path}");
    output.stdout
}

fn stage_tree(studio: &Path, root: &str, destination: &Path) {
    let output = Command::new("git")
        .arg("-C")
        .arg(studio)
        .args(["ls-tree", "-r", "--name-only", STUDIO_COMMIT, "--", root])
        .output()
        .unwrap();
    assert!(output.status.success(), "list pinned nlohmann headers");
    for path in String::from_utf8(output.stdout).unwrap().lines() {
        let relative = PathBuf::from(path).strip_prefix("src").unwrap().to_owned();
        let target = destination.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, show(studio, path)).unwrap();
    }
}

fn source_lines(source: &[u8], first: usize, last: usize) -> Vec<u8> {
    let source = String::from_utf8(source.to_vec()).unwrap();
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
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "hash pinned source excerpt");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
