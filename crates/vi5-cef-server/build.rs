fn main() -> anyhow::Result<()> {
    tonic_prost_build::configure()
        .build_server(true)
        .compile_protos(
            &[
                "../../protocol/common.proto",
                "../../protocol/lib-server.proto",
                "../../protocol/server-js.proto",
            ],
            &["../../protocol"],
        )?;
    Ok(())
}
