fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/pb")
        .build_server(true)
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .extern_path(".bookway.ad.center", "::bookway_ad_center_api::pb")
        .extern_path(".bookway.ad.rank", "::bookway_ad_rank_api::pb")
        .compile_protos(&["src/pb/server.proto"], &["src/pb", "../../"])?;
    println!("cargo:rerun-if-changed=src/pb/server.proto");
    Ok(())
}
