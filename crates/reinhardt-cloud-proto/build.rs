fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/common.proto",
                "proto/build.proto",
                "proto/cluster_agent.proto",
                "proto/log.proto",
            ],
            &["proto/"],
        )?;
    Ok(())
}
