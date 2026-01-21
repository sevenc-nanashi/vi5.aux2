fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../protocol/lib-server.proto");
    println!("cargo:rerun-if-changed=../../protocol/common.proto");
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &[
                "../../protocol/lib-server.proto",
                "../../protocol/common.proto",
            ],
            &["../../protocol"],
        )?;
    Ok(())
}
