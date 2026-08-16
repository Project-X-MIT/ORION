#[path = "../../../tests/benchmarks/leaderboard.rs"]
mod benchmark;

use std::{env, time::Duration};

use chrono::{TimeZone, Utc};
use orion_worker::jobs::leaderboard_snapshot::{
    rebuild_snapshot, run_leaderboard_snapshot, SnapshotEffectFuture, SnapshotEffects,
    SnapshotOutcome,
};
use sqlx::{postgres::PgPoolOptions, ConnectOptions, Executor, PgPool};
use uuid::Uuid;

struct NoopEffects;

impl SnapshotEffects for NoopEffects {
    fn after_commit(&self, _outcome: SnapshotOutcome) -> SnapshotEffectFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

struct TestDatabase {
    pool: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn connect() -> Option<Self> {
        let url = env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let schema = format!("orion_snapshot_{}", Uuid::new_v4().simple());
        let options = sqlx::postgres::PgConnectOptions::from_url(&url.parse().ok()?)
            .ok()?
            .options([("search_path", schema.as_str())]);
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(2))
            .connect(&url)
            .await
            .ok()?;
        admin
            .execute(format!("CREATE SCHEMA {schema}").as_str())
            .await
            .ok()?;
        admin.close().await;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(2))
            .connect_with(options)
            .await
            .ok()?;
        pool.execute(
            "CREATE TABLE users (id UUID PRIMARY KEY);
             CREATE TABLE user_ratings (
                 user_id UUID PRIMARY KEY REFERENCES users(id),
                 rating INTEGER NOT NULL
             );
             CREATE TABLE leaderboard_rank_history (
                 snapshot_at TIMESTAMPTZ NOT NULL,
                 user_id UUID NOT NULL REFERENCES users(id),
                 previous_rank BIGINT,
                 current_rank BIGINT NOT NULL,
                 rank_movement BIGINT GENERATED ALWAYS AS (previous_rank - current_rank) STORED,
                 PRIMARY KEY (snapshot_at, user_id)
             );
             CREATE TABLE outbox_events (
                 id UUID PRIMARY KEY,
                 event_type TEXT NOT NULL,
                 schema_version INTEGER NOT NULL DEFAULT 1,
                 payload JSONB NOT NULL,
                 status TEXT NOT NULL DEFAULT 'pending',
                 dispatched_at TIMESTAMPTZ,
                 job_status TEXT NOT NULL DEFAULT 'queued',
                 job_completed_at TIMESTAMPTZ,
                 lease_until TIMESTAMPTZ,
                 job_updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )
        .await
        .ok()?;
        Some(Self { pool, schema })
    }

    async fn close(self) {
        self.pool.close().await;
        if let Ok(url) = env::var("ORION_TEST_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
            if let Ok(admin) = PgPoolOptions::new().max_connections(1).connect(&url).await {
                let _ = admin
                    .execute(format!("DROP SCHEMA {} CASCADE", self.schema).as_str())
                    .await;
                admin.close().await;
            }
        }
    }
}

#[tokio::test]
async fn concurrent_retry_creates_one_logical_snapshot_and_rebuild_is_stable() {
    let Some(database) = TestDatabase::connect().await else {
        eprintln!("skipping PostgreSQL snapshot concurrency test: no test database");
        return;
    };
    for rating in [1_500_i32, 1_500, 1_200] {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id) VALUES ($1)")
            .bind(user_id)
            .execute(&database.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO user_ratings (user_id, rating) VALUES ($1, $2)")
            .bind(user_id)
            .bind(rating)
            .execute(&database.pool)
            .await
            .unwrap();
    }
    let scheduled_at = Utc.with_ymd_and_hms(2026, 8, 17, 10, 23, 0).unwrap();
    let (first, overlap) = tokio::join!(
        run_leaderboard_snapshot(&database.pool, scheduled_at, &NoopEffects),
        run_leaderboard_snapshot(&database.pool, scheduled_at, &NoopEffects)
    );
    let first = first.unwrap();
    let overlap = overlap.unwrap();

    assert_eq!(first.inserted_rows + overlap.inserted_rows, 3);
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events
         WHERE event_type = 'orion.leaderboard.snapshot.completed'",
    )
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(event_count, 1);
    let before: Vec<(Uuid, i64, Option<i64>)> = sqlx::query_as(
        "SELECT user_id, current_rank, previous_rank
         FROM leaderboard_rank_history ORDER BY current_rank",
    )
    .fetch_all(&database.pool)
    .await
    .unwrap();
    let rebuilt = rebuild_snapshot(&database.pool, scheduled_at, &NoopEffects)
        .await
        .unwrap();
    let after: Vec<(Uuid, i64, Option<i64>)> = sqlx::query_as(
        "SELECT user_id, current_rank, previous_rank
         FROM leaderboard_rank_history ORDER BY current_rank",
    )
    .fetch_all(&database.pool)
    .await
    .unwrap();

    assert_eq!(rebuilt.inserted_rows, 0);
    assert_eq!(before, after);
    database.close().await;
}
