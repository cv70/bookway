fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/pb")
        .build_server(true)
        .build_client(true)
        .extern_path(".bookway.bbs.link", "::bookway_bbs_link_api::pb")
        // Search sessions persist generated messages directly as PostgreSQL JSON.
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["src/pb/server.proto"], &["src/pb", "../.."])?;
    let _ = std::fs::remove_file("src/pb/bookway.bbs.link.rs");
    println!("cargo:rerun-if-changed=src/pb/server.proto");
    Ok(())
}
