fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
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

    let object_suffix = if target_os == "windows" { ".obj" } else { ".o" };
    let shim_object = std::fs::read_dir(&out_dir)
        .expect("shim build output exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .ends_with(&format!("-shim{object_suffix}"))
            })
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
        println!("cargo:rustc-link-arg-cdylib=-lc++");
    }
    println!("cargo:rustc-link-arg-cdylib={}", shim_object.display());
    println!("cargo:rerun-if-changed=src/shim.cpp");
}
