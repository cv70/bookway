use bookway_feature_main::{api, conf::Config, domain::Domain};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("feature-main");
    api::serve(Domain::new(Config::from_env()?).await?).await?;
    Ok(())
}
