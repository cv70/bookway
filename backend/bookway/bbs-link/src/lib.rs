pub mod api {
    pub mod pb {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/api/pb/bookway.bbs.link.rs"
        ));
    }
}
