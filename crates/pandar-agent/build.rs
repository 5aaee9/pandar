fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/pandar/agent/v1/agent.proto");
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }
    tonic_prost_build::configure().compile_protos(
        &["../../proto/pandar/agent/v1/agent.proto"],
        &["../../proto"],
    )?;
    Ok(())
}
