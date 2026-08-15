use bookway_content_audit::{api, conf::Config, domain::Domain};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("content-audit");
    api::serve(Domain::new(Config::from_env()?).await?).await?;
    Ok(())
}
