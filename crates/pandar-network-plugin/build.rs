const STUDIO_EXPORTS_PATH: &str = "src/shim_exports.hpp";

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let studio_symbols = expected_abi_symbols(&manifest_dir);

    let mut shim_build = cc::Build::new();
    shim_build.cpp(true).cargo_metadata(false);
    if target_env == "msvc" {
        shim_build
            .flag_if_supported("/std:c++17")
            .flag_if_supported("/MD")
            .define("_ITERATOR_DEBUG_LEVEL", "0");
    } else {
        shim_build
            .flag_if_supported("-std=c++17")
            .flag_if_supported("-Wno-return-type-c-linkage")
            .flag_if_supported("-Wno-unused-parameter");
    }
    shim_build
        .file("src/shim.cpp")
        .compile("pandar_network_plugin_shim");

    let shim_object = std::fs::read_dir(&out_dir)
        .expect("shim build output exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_stem()
                .is_some_and(|stem| stem.to_string_lossy().ends_with("-shim"))
                && path
                    .extension()
                    .is_some_and(|extension| matches!(extension.to_str(), Some("o" | "obj")))
        })
        .expect("cc produced shim object");
    if target_os == "linux" && target_env == "gnu" {
        let export_map = format!("{out_dir}/pandar-network-plugin.exports");
        std::fs::write(
            &export_map,
            "{
  global:
    bambu_network_*;
    ft_*;
  local:
    *;
};
",
        )
        .expect("write plugin export map");
        println!("cargo:rustc-link-arg-cdylib=-Wl,--version-script={export_map}");
        println!("cargo:rustc-link-arg-cdylib=-lstdc++");
    }
    if target_os == "macos" {
        let export_map = format!("{out_dir}/pandar-network-plugin-macos.exports");
        let export_list = studio_symbols
            .iter()
            .map(|symbol| format!("_{symbol}\n"))
            .collect::<String>();
        std::fs::write(&export_map, export_list).expect("write macOS plugin export map");
        println!("cargo:rustc-link-arg-cdylib=-Wl,-exported_symbols_list,{export_map}");
        println!("cargo:rustc-link-arg-cdylib=-lc++");
    }
    println!("cargo:rustc-link-arg-cdylib={}", shim_object.display());
    println!("cargo:rerun-if-changed=src/shim.cpp");
    for header in [
        "shim_abi_content.hpp",
        "shim_abi_operations.hpp",
        "shim_abi_user.hpp",
        "shim_account_ffi.hpp",
        "shim_connection.hpp",
        "shim_exports.hpp",
        "shim_file_transfer.hpp",
        "shim_file_transfer_types.hpp",
        "shim_firmware.hpp",
        "shim_model_task.hpp",
        "shim_model_task_types.hpp",
        "shim_no_auth.hpp",
        "shim_state.hpp",
        "shim_status.hpp",
        "shim_status_delivery.hpp",
        "shim_status_heartbeat.hpp",
        "shim_status_payload.hpp",
        "shim_profile.hpp",
        "shim_print.hpp",
        "shim_print_types.hpp",
        "shim_printer_cache.hpp",
        "shim_request_snapshot.hpp",
        "shim_session_sync.hpp",
        "shim_studio_session.hpp",
        "shim_tasks.hpp",
        "shim_types.hpp",
        "studio_materials.hpp",
    ] {
        println!("cargo:rerun-if-changed=src/{header}");
    }
}

fn expected_abi_symbols(manifest_dir: &str) -> Vec<String> {
    let path = std::path::Path::new(manifest_dir).join(STUDIO_EXPORTS_PATH);
    println!("cargo:rerun-if-changed={}", path.display());
    let content = std::fs::read_to_string(&path).expect("read reviewed Studio export map");
    let symbols = content
        .lines()
        .filter_map(|line| line.trim().strip_prefix("PANDAR_STUDIO_EXPORT("))
        .map(|record| {
            record
                .split_once(',')
                .map(|(symbol, _)| symbol.trim())
                .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
                .unwrap_or_else(|| panic!("invalid Studio export record: {record}"))
                .to_owned()
        })
        .collect::<Vec<_>>();

    let unique = symbols.iter().collect::<std::collections::BTreeSet<_>>();
    let network_count = symbols
        .iter()
        .filter(|symbol| symbol.starts_with("bambu_network_"))
        .count();
    let file_transfer_count = symbols
        .iter()
        .filter(|symbol| symbol.starts_with("ft_"))
        .count();
    assert_eq!(
        unique.len(),
        symbols.len(),
        "duplicate Studio export symbol"
    );
    assert_eq!(network_count, 108, "Studio network export count drifted");
    assert_eq!(file_transfer_count, 21, "Studio FT export count drifted");
    assert_eq!(symbols.len(), 129, "Studio export map total drifted");
    symbols
}
