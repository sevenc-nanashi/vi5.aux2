fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &["../../protocol/lib-server.proto", "../../protocol/common.proto"],
            &["../../protocol"],
        )?;
    Ok(())
}
