use std::{
    env,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use chrono::{Duration as ChronoDuration, Utc};
use orion_db::{
    models::{NewQuizAttempt, QuizType},
    pool,
    queries::quiz_attempts::{create_pending, find_by_id},
};
use orion_worker::{
    jobs::advanced_settlement::{
        settle_pending_advanced_attempt, settle_pending_advanced_attempt_with_hooks,
        settle_pending_advanced_with_retry, ActualFuture, ActualProviderError,
        AdvancedActualProvider, AdvancedAttemptContext, AdvancedPrediction, AdvancedQuestion,
        AdvancedResolution, AdvancedSettlementBoundary, AdvancedSettlementError,
        AdvancedSettlementHooks, ResolvedActual,
    },
    scheduler::RetryPolicy,
};
use rust_decimal::Decimal;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::task::JoinSet;
use uuid::Uuid;

#[derive(Clone)]
struct TestProvider {
    calls: Arc<AtomicUsize>,
    result: Result<ResolvedActual, ActualProviderError>,
}

impl AdvancedActualProvider for TestProvider {
    fn obtain_actual<'a>(&'a self, _question: &'a AdvancedQuestion) -> ActualFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

struct CrashOnce {
    boundary: AdvancedSettlementBoundary,
    fired: AtomicBool,
}

impl CrashOnce {
    fn new(boundary: AdvancedSettlementBoundary) -> Self {
        Self {
            boundary,
            fired: AtomicBool::new(false),
        }
    }
}

impl AdvancedSettlementHooks for CrashOnce {
    fn reached(&self, boundary: AdvancedSettlementBoundary) -> Result<(), AdvancedSettlementError> {
        if boundary == self.boundary && !self.fired.swap(true, Ordering::SeqCst) {
            return Err(AdvancedSettlementError::InjectedCrash(boundary));
        }
        Ok(())
    }
}

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
    user_id: Uuid,
    question_id: Uuid,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        let database_url = env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let admin = match pool::connect(&database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping Advanced settlement test: {error}");
                return None;
            }
        };
        let schema = format!("orion_worker_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated worker schema");

        let search_path = format!("SET search_path TO {schema}, public");
        let pool_search_path = search_path.clone();
        let test_pool = PgPoolOptions::new()
            .max_connections(16)
            .after_connect(move |connection, _metadata| {
                let search_path = pool_search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect isolated worker schema");
        pool::migrate(&test_pool)
            .await
            .expect("apply worker test migrations");

        let user_id = Uuid::new_v4();
        let question_id = Uuid::new_v4();
        let option_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, username, password_hash)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(format!("{user_id}@example.test"))
        .bind(format!("u_{}", user_id.simple()))
        .bind("test-password-hash")
        .execute(&test_pool)
        .await
        .expect("insert worker test user");
        sqlx::query(
            "INSERT INTO quiz_questions (id, quiz_type, category, question_text)
             VALUES ($1, 'advanced', 'test', 'Advanced settlement question')",
        )
        .bind(question_id)
        .execute(&test_pool)
        .await
        .expect("insert worker test question");
        sqlx::query(
            "INSERT INTO quiz_options (id, question_id, option_text, position, is_correct)
             VALUES ($1, $2, 'correct', 0, TRUE)",
        )
        .bind(option_id)
        .bind(question_id)
        .execute(&test_pool)
        .await
        .expect("insert worker test option");
        sqlx::query("INSERT INTO question_ratings (question_id) VALUES ($1)")
            .bind(question_id)
            .execute(&test_pool)
            .await
            .expect("insert worker question rating");

        Some(Self {
            pool: test_pool,
            admin,
            schema,
            user_id,
            question_id,
        })
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .expect("drop isolated worker schema");
        self.admin.close().await;
    }

    async fn pending_attempt(&self) -> orion_db::models::QuizAttempt {
        create_pending(
            &self.pool,
            &NewQuizAttempt::new(self.user_id, QuizType::Advanced, 1, 500),
        )
        .await
        .expect("create pending Advanced attempt")
    }

    fn context(&self, attempt_id: Uuid) -> AdvancedAttemptContext {
        let now = Utc::now();
        let question = AdvancedQuestion {
            id: self.question_id,
            value_scale: 2,
            horizon_at: now - ChronoDuration::minutes(2),
            expires_at: now + ChronoDuration::minutes(2),
        };
        AdvancedAttemptContext {
            attempt_id,
            user_id: self.user_id,
            resolutions: vec![AdvancedResolution {
                prediction: AdvancedPrediction {
                    question_id: self.question_id,
                    value: Decimal::new(100, 2),
                    submitted_at: now - ChronoDuration::minutes(3),
                },
                question,
            }],
        }
    }

    fn provider(&self, calls: Arc<AtomicUsize>) -> TestProvider {
        let now = Utc::now();
        TestProvider {
            calls,
            result: Ok(ResolvedActual {
                value: orion_worker::jobs::advanced_settlement::AdvancedActualValue {
                    question_id: self.question_id,
                    value: Decimal::new(100, 2),
                    observed_at: now - ChronoDuration::minutes(1),
                    available_at: now,
                    source_id: "test-provider".to_owned(),
                    source_version: "test-v1".to_owned(),
                    is_final: true,
                },
            }),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn one_hundred_duplicate_deliveries_create_one_settlement_and_rating_event() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let pending = database.pending_attempt().await;
    let context = database.context(pending.id);
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = database.provider(Arc::clone(&calls));
    let mut deliveries = JoinSet::new();

    for _ in 0..100 {
        let pool = database.pool.clone();
        let provider = provider.clone();
        let context = context.clone();
        deliveries.spawn(async move {
            settle_pending_advanced_attempt(&pool, &provider, &context)
                .await
                .expect("duplicate delivery settles or reads the completed result")
        });
    }
    while let Some(result) = deliveries.join_next().await {
        result.expect("duplicate delivery task does not panic");
    }

    let attempt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quiz_attempts")
        .fetch_one(&database.pool)
        .await
        .expect("count attempts");
    let rating_event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_events")
        .fetch_one(&database.pool)
        .await
        .expect("count rating events");
    let ledger_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_ledger")
        .fetch_one(&database.pool)
        .await
        .expect("count rating ledger rows");
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events
         WHERE payload ->> 'attempt_id' = $1",
    )
    .bind(pending.id.to_string())
    .fetch_one(&database.pool)
    .await
    .expect("count settlement outbox rows");
    let completed: String = sqlx::query_scalar("SELECT status FROM quiz_attempts WHERE id = $1")
        .bind(pending.id)
        .fetch_one(&database.pool)
        .await
        .expect("read completed status");

    assert_eq!(attempt_count, 1);
    assert_eq!(rating_event_count, 1);
    assert_eq!(ledger_count, 1);
    assert_eq!(outbox_count, 3);
    assert_eq!(completed, "completed");
    assert!(calls.load(Ordering::SeqCst) >= 1);

    let audit = sqlx::query_as::<_, (Decimal, Decimal, Decimal, String, i32)>(
        "SELECT advanced_prediction_value,
                advanced_actual_value,
                advanced_relative_error_pct,
                zone,
                k
         FROM rating_events
         WHERE attempt_id = $1",
    )
    .bind(pending.id)
    .fetch_one(&database.pool)
    .await
    .expect("read actual-value rating audit");
    assert_eq!(audit.0, Decimal::new(100, 2));
    assert_eq!(audit.1, Decimal::new(100, 2));
    assert_eq!(audit.2, Decimal::ZERO);
    assert_eq!(audit.3, "Win");
    assert_eq!(audit.4, 30);

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_at_each_boundary_is_recoverable_by_the_same_delivery_key() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let boundaries = [
        AdvancedSettlementBoundary::AfterPendingLookup,
        AdvancedSettlementBoundary::AfterActualValidation,
        AdvancedSettlementBoundary::BeforeAtomicSettlement,
        AdvancedSettlementBoundary::AfterAtomicSettlement,
        AdvancedSettlementBoundary::BeforeOutbox,
        AdvancedSettlementBoundary::AfterOutbox,
    ];

    for boundary in boundaries {
        let pending = database.pending_attempt().await;
        let context = database.context(pending.id);
        let provider = database.provider(Arc::new(AtomicUsize::new(0)));
        let hooks = CrashOnce::new(boundary);
        let first =
            settle_pending_advanced_attempt_with_hooks(&database.pool, &provider, &context, &hooks)
                .await;
        assert!(matches!(
            first,
            Err(AdvancedSettlementError::InjectedCrash(actual)) if actual == boundary
        ));

        settle_pending_advanced_attempt(&database.pool, &provider, &context)
            .await
            .expect("redelivery recovers the boundary crash");
        let status: String = sqlx::query_scalar("SELECT status FROM quiz_attempts WHERE id = $1")
            .bind(pending.id)
            .fetch_one(&database.pool)
            .await
            .expect("read recovered status");
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM rating_events WHERE attempt_id = $1")
                .bind(pending.id)
                .fetch_one(&database.pool)
                .await
                .expect("count recovered rating event");
        let outbox_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE payload ->> 'attempt_id' = $1",
        )
        .bind(pending.id.to_string())
        .fetch_one(&database.pool)
        .await
        .expect("count recovered outbox rows");

        assert_eq!(status, "completed");
        assert_eq!(event_count, 1);
        assert_eq!(outbox_count, 3);
    }

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_outage_retries_boundedly_and_leaves_attempt_pending() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let pending = database.pending_attempt().await;
    let context = database.context(pending.id);
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
        result: Err(ActualProviderError::Unavailable),
    };
    let policy = RetryPolicy {
        max_attempts: 3,
        initial_delay: Duration::ZERO,
        maximum_delay: Duration::ZERO,
        jitter_percent: 0,
    };

    let outcome = settle_pending_advanced_with_retry(&database.pool, &provider, &context, policy)
        .await
        .expect("provider outage reaches the durable dead-letter outcome");
    assert!(matches!(
        outcome,
        orion_worker::jobs::advanced_settlement::AdvancedSettlementOutcome::DeadLettered
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    let attempt = find_by_id(&database.pool, pending.id)
        .await
        .expect("read pending attempt")
        .expect("pending attempt remains present");
    let rating_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rating_events WHERE attempt_id = $1")
            .bind(pending.id)
            .fetch_one(&database.pool)
            .await
            .expect("count outage rating events");
    let dead_letters: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events
         WHERE event_type = 'orion.quiz.advanced.settlement.dead_lettered'
           AND payload ->> 'attempt_id' = $1",
    )
    .bind(pending.id.to_string())
    .fetch_one(&database.pool)
    .await
    .expect("count outage dead-letter event");

    assert_eq!(attempt.status, "pending");
    assert_eq!(rating_events, 0);
    assert_eq!(dead_letters, 1);

    database.cleanup().await;
}
