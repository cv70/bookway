use bookway_recommend_recall::{api, conf::Config, domain::Domain};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("recommend-recall");
    api::serve(Domain::new(Config::from_env()?).await?).await?;
    Ok(())
}
