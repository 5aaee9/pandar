const STUDIO_EXPORTS_PATH: &str = "src/shim_exports.hpp";

fn main() {
    let studio_profile = pandar_studio_profile::profile_from_env()
        .unwrap_or_else(|error| panic!("select Studio ABI profile: {error}"));
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let studio_symbols = expected_abi_symbols(&manifest_dir, studio_profile);

    println!(
        "cargo:rerun-if-env-changed={}",
        pandar_studio_profile::PROFILE_ENV
    );
    println!("cargo:rerun-if-changed=../../studio-abi-profiles.json");
    println!(
        "cargo:rustc-env=PANDAR_STUDIO_PROFILE_ID={}",
        studio_profile.id
    );
    println!(
        "cargo:rustc-env=PANDAR_NETWORK_AGENT_VERSION={}",
        studio_profile.network_agent_version
    );
    println!("cargo:rustc-check-cfg=cfg(pandar_studio_ams_sync)");

    let mut shim_build = cc::Build::new();
    shim_build.cpp(true).cargo_metadata(false);
    if studio_profile.capabilities.bind_model_argument {
        shim_build.define("PANDAR_STUDIO_BIND_MODEL_ARGUMENT", None);
    }
    if studio_profile.capabilities.print_slicer_uid {
        shim_build.define("PANDAR_STUDIO_PRINT_SLICER_UID", None);
    }
    if studio_profile.capabilities.ams_sync {
        shim_build.define("PANDAR_STUDIO_AMS_SYNC", None);
        println!("cargo:rustc-cfg=pandar_studio_ams_sync");
    }
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

fn expected_abi_symbols(
    manifest_dir: &str,
    profile: &pandar_studio_profile::StudioProfile,
) -> Vec<String> {
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
        .filter(|symbol| {
            profile.capabilities.ams_sync || symbol != "bambu_network_sync_ams_filaments"
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
    assert_eq!(
        network_count, profile.network_exports,
        "Studio network export count drifted"
    );
    assert_eq!(
        file_transfer_count, profile.file_transfer_exports,
        "Studio FT export count drifted"
    );
    assert_eq!(
        symbols.len(),
        profile.total_exports(),
        "Studio export map total drifted"
    );
    symbols
}
