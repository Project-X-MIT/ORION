use std::env;

use chrono::{Duration, Utc};
use orion_db::{
    pool,
    queries::outbox::{claim_batch, fail, replay},
    transactions::{write_outbox_event, write_outbox_event_with_context},
};
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

    let request_id = Uuid::new_v4();
    let mut contextual = database.pool.begin().await.expect("begin context test");
    let contextual_id = write_outbox_event_with_context(
        &mut contextual,
        "orion.notification.requested",
        1,
        serde_json::json!({"recipient_id": Uuid::new_v4()}),
        Some(request_id),
        Some("trace-outbox-test"),
    )
    .await
    .expect("write contextual event");
    contextual.commit().await.expect("commit contextual event");
    let context = sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(
        "SELECT request_id, trace_id FROM outbox_events WHERE id = $1",
    )
    .bind(contextual_id)
    .fetch_one(&database.pool)
    .await
    .expect("read contextual event");
    assert_eq!(context.0, Some(request_id));
    assert_eq!(context.1.as_deref(), Some("trace-outbox-test"));

    database.cleanup().await;
}

#[tokio::test]
async fn outbox_claim_lease_retry_dead_letter_and_replay_preserve_identity() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    pool::migrate(&database.pool)
        .await
        .expect("apply complete migration chain");

    let event_id: Uuid = sqlx::query_scalar(
        "INSERT INTO outbox_events
            (event_type, schema_version, payload, request_id, trace_id)
         VALUES ('orion.synthetic.v1', 1, '{\"run\":\"lease-drill\"}', $1, $2)
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind("trace-lease-drill")
    .fetch_one(&database.pool)
    .await
    .expect("insert synthetic outbox event");

    let claimed = claim_batch(&database.pool, 1, 30)
        .await
        .expect("claim first lease");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, event_id);
    assert_eq!(claimed[0].job_attempts, 1);

    let competing_claim = claim_batch(&database.pool, 1, 30)
        .await
        .expect("claim competing lease");
    assert!(competing_claim.is_empty(), "active lease was double-owned");

    sqlx::query(
        "UPDATE outbox_events
         SET lease_until = CURRENT_TIMESTAMP - INTERVAL '1 second'
         WHERE id = $1",
    )
    .bind(event_id)
    .execute(&database.pool)
    .await
    .expect("expire synthetic lease");
    let recovered = claim_batch(&database.pool, 1, 30)
        .await
        .expect("recover expired lease");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, event_id);
    assert_eq!(recovered[0].job_attempts, 2);

    fail(
        &database.pool,
        event_id,
        "synthetic_poison_event",
        2,
        Utc::now() + Duration::seconds(1),
    )
    .await
    .expect("dead-letter poison event");
    let state = sqlx::query_as::<_, (String, String, i32, Option<Uuid>, Option<String>)>(
        "SELECT status, job_status, job_attempts, request_id, trace_id
         FROM outbox_events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(&database.pool)
    .await
    .expect("read dead-letter state");
    assert_eq!(state.0, "pending");
    assert_eq!(state.1, "dead_letter");
    assert_eq!(state.2, 2);
    assert!(state.3.is_some());
    assert_eq!(state.4.as_deref(), Some("trace-lease-drill"));

    assert!(replay(&database.pool, event_id)
        .await
        .expect("replay dead letter"));
    let replayed = sqlx::query_as::<_, (Uuid, String, String, i32, Option<String>)>(
        "SELECT id, status, job_status, job_attempts, trace_id
         FROM outbox_events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(&database.pool)
    .await
    .expect("read replay state");
    assert_eq!(replayed.0, event_id);
    assert_eq!(replayed.1, "pending");
    assert_eq!(replayed.2, "queued");
    assert_eq!(replayed.3, 2);
    assert_eq!(replayed.4.as_deref(), Some("trace-lease-drill"));

    database.cleanup().await;
}
