fn main() {
    println!("cargo:rerun-if-changed=../../studio-abi-profiles.json");
    println!("cargo:rerun-if-changed=src/hook.cpp");
    println!("cargo:rerun-if-changed=src/plugin_download_hook.cpp");
    println!("cargo:rerun-if-changed=src/plugin_download_hook.hpp");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").expect("target env is set by Cargo");
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let abi_series_header = std::path::Path::new(&out_dir).join("pandar_studio_abi_series.hpp");
    let abi_series = pandar_studio_profile::catalog()
        .abi_series
        .iter()
        .map(|series| {
            let [major, minor, patch] = series.series_components();
            format!("    {{{major}, {minor}, {patch}, L\"{}\"}},\n", series.id)
        })
        .collect::<String>();
    std::fs::write(
        &abi_series_header,
        format!(
            "#pragma once\n\nstruct PandarStudioAbiSeries {{ unsigned short major; unsigned short minor; unsigned short patch; const wchar_t* id; }};\nconstexpr PandarStudioAbiSeries kPandarStudioAbiSeries[] = {{\n{abi_series}}};\n"
        ),
    )
    .expect("write generated Studio ABI series header");
    let target = std::env::var("TARGET").expect("TARGET is set by Cargo");
    let cl = cc::windows_registry::find_tool(&target, "cl.exe")
        .expect("MSVC cl.exe is available for Windows hook builds");
    let mut objects = Vec::new();
    for source in ["hook.cpp", "plugin_download_hook.cpp"] {
        let object = std::path::Path::new(&out_dir).join(format!("{source}.obj"));
        let status = cl
            .to_command()
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg(format!("/I{out_dir}"))
            .arg("/c")
            .arg(format!("src/{source}"))
            .arg(format!("/Fo{}", object.display()))
            .status()
            .expect("run cl.exe for Studio hook DLL");
        assert!(status.success(), "compile Studio hook DLL object {source}");
        objects.push(object);
    }

    let profile_dir = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is under target profile directory");
    let dll_path = profile_dir.join("pandar_studio_hook.dll");
    let import_lib = std::path::Path::new(&out_dir).join("pandar_studio_hook.lib");

    let link = cc::windows_registry::find_tool(&target, "link.exe")
        .expect("MSVC link.exe is available for Windows hook builds");
    let status = link
        .to_command()
        .arg("/NOLOGO")
        .arg("/DLL")
        .arg(format!("/OUT:{}", dll_path.display()))
        .arg(format!("/IMPLIB:{}", import_lib.display()))
        .args(objects)
        .arg("kernel32.lib")
        .arg("version.lib")
        .status()
        .expect("run link.exe for Studio hook DLL");
    assert!(status.success(), "link Studio hook DLL");
}
