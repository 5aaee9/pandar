fn main() {
    println!("cargo:rerun-if-changed=src/hook.cpp");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("target OS is set by Cargo");
    if target_os != "windows" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let target = std::env::var("TARGET").expect("TARGET is set by Cargo");

    let object = std::path::Path::new(&out_dir).join("hook.obj");
    let cl = cc::windows_registry::find_tool(&target, "cl.exe")
        .expect("MSVC cl.exe is available for Windows hook builds");
    let status = cl
        .to_command()
        .arg("/nologo")
        .arg("/std:c++17")
        .arg("/EHsc")
        .arg("/c")
        .arg("src/hook.cpp")
        .arg(format!("/Fo{}", object.display()))
        .status()
        .expect("run cl.exe for Studio dev hook DLL");
    assert!(status.success(), "compile Studio dev hook DLL object");

    let profile_dir = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is under target profile directory");
    let dll_path = profile_dir.join("pandar_studio_dev_hook.dll");
    let import_lib = std::path::Path::new(&out_dir).join("pandar_studio_dev_hook.lib");

    let link = cc::windows_registry::find_tool(&target, "link.exe")
        .expect("MSVC link.exe is available for Windows hook builds");
    let status = link
        .to_command()
        .arg("/NOLOGO")
        .arg("/DLL")
        .arg(format!("/OUT:{}", dll_path.display()))
        .arg(format!("/IMPLIB:{}", import_lib.display()))
        .arg(object)
        .arg("kernel32.lib")
        .status()
        .expect("run link.exe for Studio dev hook DLL");
    assert!(status.success(), "link Studio dev hook DLL");
}
