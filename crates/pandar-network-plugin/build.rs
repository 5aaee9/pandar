const ABI_SYMBOLS_PATH: &str =
    "../../docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt";

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    cc::Build::new()
        .cpp(true)
        .cargo_metadata(false)
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-Wno-return-type-c-linkage")
        .flag_if_supported("-Wno-unused-parameter")
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
        let symbols = expected_abi_symbols(&manifest_dir);
        let export_map = format!("{out_dir}/pandar-network-plugin-macos.exports");
        let export_list = symbols
            .iter()
            .map(|symbol| format!("_{symbol}\n"))
            .collect::<String>();
        std::fs::write(&export_map, export_list).expect("write macOS plugin export map");
        println!("cargo:rustc-link-arg-cdylib=-Wl,-exported_symbols_list,{export_map}");
        println!("cargo:rustc-link-arg-cdylib=-lc++");
    }
    println!("cargo:rustc-link-arg-cdylib={}", shim_object.display());
    if target_os == "windows" {
        println!("cargo:rustc-link-arg-cdylib=-lc++");
        println!("cargo:rustc-link-arg-cdylib=-lc++abi");
    }
    println!("cargo:rerun-if-changed=src/shim.cpp");
}

fn expected_abi_symbols(manifest_dir: &str) -> Vec<String> {
    let path = std::path::Path::new(manifest_dir).join(ABI_SYMBOLS_PATH);
    println!("cargo:rerun-if-changed={}", path.display());
    let content = std::fs::read_to_string(&path).expect("read network plugin ABI symbols");
    let symbols = content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("bambu_network_") || line.starts_with("ft_"))
        .map(str::to_owned)
        .collect::<Vec<_>>();

    assert!(
        !symbols.is_empty(),
        "network plugin ABI symbols are not empty"
    );
    symbols
}
