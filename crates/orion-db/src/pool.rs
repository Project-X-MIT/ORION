use sqlx::{postgres::PgPoolOptions, PgPool};

/// Creates a PostgreSQL pool for the DB repositories and transactions.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().connect(database_url).await
}
