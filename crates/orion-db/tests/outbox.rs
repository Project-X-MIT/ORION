use std::env;

use orion_db::{pool, transactions::write_outbox_event};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        let database_url = env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let admin = match pool::connect(&database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping outbox test: {error}");
                return None;
            }
        };
        let schema = format!("orion_outbox_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated outbox schema");

        let search_path = format!("SET search_path TO {schema}, public");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _metadata| {
                let search_path = search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect isolated outbox schema");

        Some(Self {
            pool,
            admin,
            schema,
        })
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .expect("drop isolated outbox schema");
        self.admin.close().await;
    }
}

#[tokio::test]
async fn outbox_write_is_transactional_and_serializes_generic_payloads() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    pool::migrate(&database.pool)
        .await
        .expect("apply complete migration chain");

    let mut rolled_back = database.pool.begin().await.expect("begin rollback test");
    write_outbox_event(
        &mut rolled_back,
        "research_elo_award",
        serde_json::json!({"paper_id": Uuid::new_v4(), "score": 4.5}),
    )
    .await
    .expect("write event in transaction");
    rolled_back.rollback().await.expect("roll back event");

    let rolled_back_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events")
        .fetch_one(&database.pool)
        .await
        .expect("count rolled-back events");
    assert_eq!(rolled_back_count, 0);

    let mut committed = database.pool.begin().await.expect("begin commit test");
    let event_id = write_outbox_event(
        &mut committed,
        "notification",
        serde_json::json!({"recipient_id": Uuid::new_v4(), "body": "ready"}),
    )
    .await
    .expect("write committed event");
    committed.commit().await.expect("commit event");

    let event = sqlx::query_as::<_, (String, serde_json::Value, String, i32)>(
        "SELECT event_type, payload, status, retry_count
         FROM outbox_events
         WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(&database.pool)
    .await
    .expect("read committed event");
    assert_eq!(event.0, "notification");
    assert_eq!(event.1["body"], "ready");
    assert_eq!(event.2, "pending");
    assert_eq!(event.3, 0);

    database.cleanup().await;
}
