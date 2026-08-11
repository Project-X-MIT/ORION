use std::{collections::HashSet, env, error::Error, time::Duration as StdDuration};

use orion_api::routes::leaderboard::{LeaderboardService, RankService};
use orion_db::{pool as db_pool, transactions::snapshot_leaderboard};
use orion_redis::RedisClient;
use sqlx::{
    postgres::PgPoolOptions,
    types::chrono::{DateTime, Utc},
    PgPool,
};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Result<Option<Self>, Box<dyn Error>> {
        let database_url = match env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
        {
            Ok(database_url) => database_url,
            Err(_) => {
                eprintln!("Skipping leaderboard integration test: no PostgreSQL URL configured");
                return Ok(None);
            }
        };
        let admin = db_pool::connect(&database_url).await?;
        let schema = format!("orion_leaderboard_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await?;
        let search_path = format!("SET search_path TO {schema}, public");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |connection, _| {
                let search_path = search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await?;
        db_pool::migrate(&pool).await?;
        Ok(Some(Self {
            pool,
            admin,
            schema,
        }))
    }

    async fn cleanup(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await?;
        self.admin.close().await;
        Ok(())
    }
}

fn user_id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

async fn seed_ranked_users(pool: &PgPool) -> Result<(), sqlx::Error> {
    for (id, username, rating) in [
        (user_id(1), "alpha", 1_600),
        (user_id(2), "bravo", 1_500),
        (user_id(3), "charlie", 1_500),
        (user_id(4), "delta", 1_400),
        (user_id(5), "echo", 1_300),
    ] {
        sqlx::query(
            "INSERT INTO users (id, email, username, password_hash)
             VALUES ($1, $2, $3, '$argon2id$leaderboard-test')",
        )
        .bind(id)
        .bind(format!("{username}@leaderboard.test"))
        .bind(username)
        .execute(pool)
        .await?;
        sqlx::query("INSERT INTO user_ratings (user_id, rating) VALUES ($1, $2)")
            .bind(id)
            .bind(rating)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn snapshot_time(pool: &PgPool, minutes_ago: i32) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar("SELECT CURRENT_TIMESTAMP - ($1 * INTERVAL '1 minute')")
        .bind(minutes_ago)
        .fetch_one(pool)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orders_by_elo_then_uuid_and_exposes_movement() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    seed_ranked_users(&database.pool).await?;
    let first_snapshot = snapshot_time(&database.pool, 2).await?;
    snapshot_leaderboard(&database.pool, first_snapshot).await?;
    sqlx::query("UPDATE user_ratings SET rating = 1700 WHERE user_id = $1")
        .bind(user_id(4))
        .execute(&database.pool)
        .await?;
    let second_snapshot = snapshot_time(&database.pool, 1).await?;
    snapshot_leaderboard(&database.pool, second_snapshot).await?;

    let page = RankService::new(database.pool.clone())
        .global_page(10, None)
        .await?;
    let actual: Vec<_> = page
        .entries
        .iter()
        .map(|entry| entry.user_id.into_uuid())
        .collect();
    assert_eq!(
        actual,
        vec![user_id(4), user_id(1), user_id(2), user_id(3), user_id(5)]
    );
    assert_eq!(page.entries[0].rank, 1);
    assert_eq!(page.entries[0].rating.get(), 1_700);
    assert_eq!(page.entries[0].rank_movement, Some(3));
    assert_eq!(page.entries[2].rating, page.entries[3].rating);
    assert!(page.entries[2].user_id.into_uuid() < page.entries[3].user_id.into_uuid());

    database.cleanup().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opaque_cursor_pages_without_duplicates_or_omissions() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    seed_ranked_users(&database.pool).await?;
    let service = RankService::new(database.pool.clone());

    let first = service.global_page(2, None).await?;
    let second = service.global_page(2, first.next_cursor.as_deref()).await?;
    let third = service
        .global_page(2, second.next_cursor.as_deref())
        .await?;
    assert!(third.next_cursor.is_none());

    let ids: Vec<_> = first
        .entries
        .into_iter()
        .chain(second.entries)
        .chain(third.entries)
        .map(|entry| entry.user_id.into_uuid())
        .collect();
    assert_eq!(ids.len(), 5);
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 5);
    assert_eq!(
        ids,
        vec![user_id(1), user_id(2), user_id(3), user_id(4), user_id(5)]
    );

    database.cleanup().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redis_outage_mode_falls_back_to_authoritative_postgres() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    seed_ranked_users(&database.pool).await?;

    let service = match env::var("ORION_TEST_REDIS_URL").or_else(|_| env::var("REDIS_URL")) {
        Ok(redis_url) => {
            let redis = RedisClient::connect(&redis_url, StdDuration::from_secs(2)).await?;
            let service = LeaderboardService::with_cache(database.pool.clone(), redis.clone());
            // Stop Redis from the client's perspective after constructing the
            // cache-aware service. Its failed GET must degrade to PostgreSQL.
            redis.close().await?;
            service
        }
        Err(_) => LeaderboardService::without_cache(database.pool.clone()),
    };
    let page = service.global_page(3, None).await?;
    assert_eq!(page.entries.len(), 3);
    assert_eq!(page.entries[0].user_id.into_uuid(), user_id(1));
    assert_eq!(page.entries[0].rating.get(), 1_600);

    database.cleanup().await
}
