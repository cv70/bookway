fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/pb")
        .build_server(true)
        .build_client(true)
        .extern_path(".bookway.bbs.search", "::bookway_bbs_search_api::pb")
        .extern_path(".bookway.bbs.link", "::bookway_bbs_link_api::pb")
        .compile_protos(&["src/pb/server.proto"], &["src/pb", "../.."])?;
    // The imported package is linked through extern_path and is not part of this API crate.
    let _ = std::fs::remove_file("src/pb/bookway.bbs.search.rs");
    let _ = std::fs::remove_file("src/pb/bookway.bbs.link.rs");
    println!("cargo:rerun-if-changed=src/pb/server.proto");
    Ok(())
}
