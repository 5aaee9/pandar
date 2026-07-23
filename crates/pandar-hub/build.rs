fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/pandar/agent/v1/agent.proto");
    println!("cargo:rerun-if-changed=../../proto/pandar/agent/v1/firmware.proto");
    println!("cargo:rerun-if-changed=../../proto/pandar/agent/v1/print_project.proto");
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }
    tonic_prost_build::configure()
        .type_attribute(
            ".pandar.agent.v1.AgentEvent.event",
            "#[allow(clippy::large_enum_variant)]",
        )
        .type_attribute(
            ".pandar.agent.v1.HubCommand.command",
            "#[allow(clippy::large_enum_variant)]",
        )
        .compile_protos(
            &[
                "../../proto/pandar/agent/v1/agent.proto",
                "../../proto/pandar/agent/v1/firmware.proto",
                "../../proto/pandar/agent/v1/print_project.proto",
            ],
            &["../../proto"],
        )?;
    Ok(())
}
