use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("build_descriptor.bin"))
        .compile_protos(&["proto/build.proto"], &["proto/"])?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("cluster_agent_descriptor.bin"))
        .compile_protos(&["proto/cluster_agent.proto"], &["proto/"])?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("log_descriptor.bin"))
        .compile_protos(&["proto/log.proto"], &["proto/"])?;

    // Common is compiled as part of the above (imported), but compile
    // separately to ensure its module is generated.
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/common.proto"], &["proto/"])?;

    Ok(())
}
