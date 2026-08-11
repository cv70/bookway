use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = bookway_data::postgres_pool().await?;
    MIGRATOR.run(&pool).await?;
    println!("database migrations applied");
    Ok(())
}
