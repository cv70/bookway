use bookway_recommend_rank::{api, conf::Config, domain::Domain};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("recommend-rank");
    api::serve(Domain::new(Config::from_env()?)).await?;
    Ok(())
}
