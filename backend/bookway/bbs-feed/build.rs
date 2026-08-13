fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/api/pb")
        .build_server(true)
        .build_client(true)
        .compile_protos(&["src/api/pb/server.proto"], &["src/api/pb"])?;
    println!("cargo:rerun-if-changed=src/api/pb/server.proto");
    Ok(())
}
