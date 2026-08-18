fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/pb")
        .build_server(true)
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["src/pb/server.proto"], &["src/pb"])?;
    println!("cargo:rerun-if-changed=src/pb/server.proto");
    Ok(())
}
