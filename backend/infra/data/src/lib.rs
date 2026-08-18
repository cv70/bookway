use std::{env, str::FromStr, time::Duration};

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use sqlx::{PgPool, postgres::PgPoolOptions};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StorageMode {
    #[default]
    Memory,
    Postgres,
}

impl FromStr for StorageMode {
    type Err = DataError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "memory" => Ok(Self::Memory),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            value => Err(DataError::InvalidStorageMode(value.to_string())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DataError {
    #[error("invalid STORAGE_MODE: {0}; expected memory or postgres")]
    InvalidStorageMode(String),
    #[error("DATABASE_URL is required when STORAGE_MODE=postgres")]
    MissingDatabaseUrl,
    #[error("invalid database pool setting {key}: {value}")]
    InvalidPoolSetting { key: &'static str, value: String },
    #[error("postgres connection failed: {0}")]
    Postgres(#[from] sqlx::Error),
    #[error("redis connection failed: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("redis connection timed out after {0}ms")]
    RedisTimeout(u64),
}

pub fn storage_mode() -> Result<StorageMode, DataError> {
    env::var("STORAGE_MODE")
        .unwrap_or_else(|_| "memory".to_string())
        .parse()
}

pub async fn postgres_pool() -> Result<PgPool, DataError> {
    let database_url = env::var("DATABASE_URL").map_err(|_| DataError::MissingDatabaseUrl)?;
    let max_connections = env_u32("DATABASE_MAX_CONNECTIONS", 20)?;
    let min_connections = env_u32("DATABASE_MIN_CONNECTIONS", 1)?;
    let acquire_timeout = env_u64("DATABASE_ACQUIRE_TIMEOUT_SECONDS", 5)?;

    Ok(PgPoolOptions::new()
        .min_connections(min_connections)
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout))
        .connect(&database_url)
        .await?)
}

pub async fn redis_connection() -> Result<Option<ConnectionManager>, DataError> {
    let Ok(redis_url) = env::var("REDIS_URL") else {
        return Ok(None);
    };
    let client = redis::Client::open(redis_url)?;
    let timeout_ms = env_u64("REDIS_CONNECT_TIMEOUT_MS", 1_000)?;
    let command_timeout_ms = env_u64("REDIS_COMMAND_TIMEOUT_MS", 100)?;
    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Duration::from_millis(timeout_ms))
        .set_response_timeout(Duration::from_millis(command_timeout_ms));
    let manager = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        ConnectionManager::new_with_config(client, config),
    )
    .await
    .map_err(|_| DataError::RedisTimeout(timeout_ms))??;
    Ok(Some(manager))
}

fn env_u32(key: &'static str, default: u32) -> Result<u32, DataError> {
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| DataError::InvalidPoolSetting { key, value }),
        Err(_) => Ok(default),
    }
}

fn env_u64(key: &'static str, default: u64) -> Result<u64, DataError> {
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| DataError::InvalidPoolSetting { key, value }),
        Err(_) => Ok(default),
    }
}
