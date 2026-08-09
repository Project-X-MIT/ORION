use std::time::Duration;

use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool};
use thiserror::Error;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
const USERS_COMPATIBILITY_PREFLIGHT: &str =
    include_str!("../migrations/202608090010_users_foundation.sql");

#[derive(Debug, Clone)]
pub struct PoolConfig<'a> {
    pub database_url: &'a str,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
}

impl<'a> PoolConfig<'a> {
    #[must_use]
    pub fn new(database_url: &'a str) -> Self {
        Self {
            database_url,
            max_connections: 10,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseStartupError {
    #[error("could not connect to PostgreSQL")]
    Connection(#[source] sqlx::Error),
    #[error("could not prepare the legacy users migration prerequisite")]
    CompatibilityPreflight(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database health validation failed")]
    Health(#[source] sqlx::Error),
}

/// Creates a PostgreSQL pool for repositories and transactions.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    connect_with_config(&PoolConfig::new(database_url)).await
}

pub async fn connect_with_config(config: &PoolConfig<'_>) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .connect(config.database_url)
        .await
}

/// Applies the immutable migration chain while repairing the historical empty
/// DB-01 slot. The preflight SQL is idempotent and is later recorded by SQLx as
/// migration 202608090010 in normal version order.
pub async fn migrate(pool: &PgPool) -> Result<(), DatabaseStartupError> {
    // PostgreSQL's CREATE EXTENSION IF NOT EXISTS can still race in catalog
    // insertion when multiple application instances boot simultaneously.
    // Serialize only the short compatibility preflight transaction.
    let mut preflight = pool
        .begin()
        .await
        .map_err(DatabaseStartupError::CompatibilityPreflight)?;
    sqlx::query("SELECT pg_advisory_xact_lock(675210482741)")
        .execute(&mut *preflight)
        .await
        .map_err(DatabaseStartupError::CompatibilityPreflight)?;
    let migration_table_exists: bool = sqlx::query_scalar(
        "SELECT to_regclass(current_schema() || '._sqlx_migrations') IS NOT NULL",
    )
    .fetch_one(&mut *preflight)
    .await
    .map_err(DatabaseStartupError::CompatibilityPreflight)?;
    let users_foundation_applied = if migration_table_exists {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM _sqlx_migrations
                WHERE version = 202608090010 AND success = TRUE
            )",
        )
        .fetch_one(&mut *preflight)
        .await
        .map_err(DatabaseStartupError::CompatibilityPreflight)?
    } else {
        false
    };
    if !users_foundation_applied {
        sqlx::raw_sql(USERS_COMPATIBILITY_PREFLIGHT)
            .execute(&mut *preflight)
            .await
            .map_err(DatabaseStartupError::CompatibilityPreflight)?;
    }
    preflight
        .commit()
        .await
        .map_err(DatabaseStartupError::CompatibilityPreflight)?;
    MIGRATOR
        .run(pool)
        .await
        .map_err(DatabaseStartupError::Migration)
}

pub async fn health_check(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map(|_| ())
}

pub async fn connect_migrate_and_validate(
    config: &PoolConfig<'_>,
) -> Result<PgPool, DatabaseStartupError> {
    let pool = connect_with_config(config)
        .await
        .map_err(DatabaseStartupError::Connection)?;
    migrate(&pool).await?;
    health_check(&pool)
        .await
        .map_err(DatabaseStartupError::Health)?;
    Ok(pool)
}
