use std::env;

use orion_db::{
    models::{NewQuizAttempt, QuizAnswer, QuizSettlementInput, QuizType},
    pool,
    queries::quiz_attempts::{create_pending, find_completed_by_id, pending_advanced_by_user_id},
    transactions::{settle_advanced_quiz, settle_basic_quiz},
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
    user_id: Uuid,
    question_id: Uuid,
    option_id: Uuid,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        let database_url = env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let admin = match pool::connect(&database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping PostgreSQL concurrency test: {error}");
                return None;
            }
        };
        let schema = format!("orion_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated test schema");

        create_schema_objects(&admin, &schema).await;

        let search_path = format!("SET search_path TO {schema}, public");
        let connection_search_path = search_path.clone();
        let test_pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _metadata| {
                let search_path = connection_search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect test pool");

        let user_id = Uuid::new_v4();
        let question_id = Uuid::new_v4();
        let option_id = Uuid::new_v4();
        let wrong_option_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id) VALUES ($1)")
            .bind(user_id)
            .execute(&test_pool)
            .await
            .expect("insert test user");
        sqlx::query(
            "INSERT INTO quiz_questions (id, quiz_type, category, question_text) \
             VALUES ($1, 'basic', 'test', 'Concurrent settlement question')",
        )
        .bind(question_id)
        .execute(&test_pool)
        .await
        .expect("insert test question");
        sqlx::query(
            "INSERT INTO quiz_options (id, question_id, option_text, position, is_correct) \
             VALUES ($1, $2, 'correct', 0, TRUE), ($3, $2, 'wrong', 1, FALSE)",
        )
        .bind(option_id)
        .bind(question_id)
        .bind(wrong_option_id)
        .execute(&test_pool)
        .await
        .expect("insert test options");
        sqlx::query("INSERT INTO question_ratings (question_id) VALUES ($1)")
            .bind(question_id)
            .execute(&test_pool)
            .await
            .expect("insert test question rating");

        Some(Self {
            pool: test_pool,
            admin,
            schema,
            user_id,
            question_id,
            option_id,
        })
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .expect("drop isolated test schema");
        self.admin.close().await;
    }
}

async fn create_schema_objects(admin: &PgPool, schema: &str) {
    let statements = [
        format!("CREATE TABLE {schema}.users (id UUID PRIMARY KEY)"),
        format!(
            "CREATE TABLE {schema}.user_ratings (\
                user_id UUID PRIMARY KEY REFERENCES {schema}.users(id),\
                rating INTEGER NOT NULL DEFAULT 500,\
                games_played INTEGER NOT NULL DEFAULT 0,\
                wins INTEGER NOT NULL DEFAULT 0,\
                losses INTEGER NOT NULL DEFAULT 0,\
                draws INTEGER NOT NULL DEFAULT 0,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
        format!(
            "CREATE TABLE {schema}.quiz_questions (\
                id UUID PRIMARY KEY,\
                quiz_type TEXT NOT NULL,\
                category TEXT NOT NULL,\
                question_text TEXT NOT NULL,\
                explanation TEXT,\
                active BOOLEAN NOT NULL DEFAULT TRUE,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
        format!(
            "CREATE TABLE {schema}.quiz_options (\
                id UUID PRIMARY KEY,\
                question_id UUID NOT NULL REFERENCES {schema}.quiz_questions(id),\
                option_text TEXT NOT NULL,\
                position INTEGER NOT NULL,\
                is_correct BOOLEAN NOT NULL,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
        format!(
            "CREATE TABLE {schema}.question_ratings (\
                question_id UUID PRIMARY KEY REFERENCES {schema}.quiz_questions(id),\
                rating INTEGER NOT NULL DEFAULT 500,\
                attempts INTEGER NOT NULL DEFAULT 0,\
                correct_answers INTEGER NOT NULL DEFAULT 0,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
        format!(
            "CREATE TABLE {schema}.quiz_attempts (\
                id UUID PRIMARY KEY,\
                user_id UUID NOT NULL REFERENCES {schema}.users(id),\
                quiz_type TEXT NOT NULL,\
                status TEXT NOT NULL,\
                total_questions INTEGER NOT NULL,\
                correct_answers INTEGER NOT NULL,\
                score INTEGER NOT NULL,\
                rating_before INTEGER NOT NULL,\
                rating_after INTEGER NOT NULL,\
                started_at TIMESTAMPTZ NOT NULL,\
                completed_at TIMESTAMPTZ,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
        format!(
            "CREATE TABLE {schema}.rating_events (\
                id UUID PRIMARY KEY,\
                attempt_id UUID REFERENCES {schema}.quiz_attempts(id),\
                user_id UUID NOT NULL REFERENCES {schema}.users(id),\
                question_id UUID NOT NULL REFERENCES {schema}.quiz_questions(id),\
                source_type TEXT NOT NULL,\
                source_id UUID NOT NULL,\
                quiz_type TEXT NOT NULL,\
                outcome SMALLINT NOT NULL,\
                correct BOOLEAN NOT NULL,\
                zone TEXT NOT NULL,\
                error_pct DOUBLE PRECISION NOT NULL,\
                k INTEGER NOT NULL,\
                sa DOUBLE PRECISION NOT NULL,\
                point_delta INTEGER NOT NULL,\
                user_rating_before INTEGER NOT NULL,\
                user_rating_after INTEGER NOT NULL,\
                player_elo_before INTEGER NOT NULL,\
                player_elo_after INTEGER NOT NULL,\
                question_rating_before INTEGER NOT NULL,\
                question_rating_after INTEGER NOT NULL,\
                question_elo_before INTEGER NOT NULL,\
                question_elo_after INTEGER NOT NULL,\
                rating_delta INTEGER NOT NULL,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                UNIQUE (attempt_id, question_id))"
        ),
        format!(
            "CREATE TABLE {schema}.rating_ledger (\
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),\
                user_id UUID NOT NULL REFERENCES {schema}.users(id),\
                source_type TEXT NOT NULL,\
                source_id UUID NOT NULL,\
                dedupe_key TEXT NOT NULL,\
                rating_before INTEGER NOT NULL,\
                rating_after INTEGER NOT NULL,\
                rating_delta INTEGER NOT NULL,\
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                UNIQUE (source_type, source_id, dedupe_key),\
                CHECK (rating_delta = rating_after - rating_before))"
        ),
    ];

    for statement in statements {
        sqlx::query(&statement)
            .execute(admin)
            .await
            .expect("create isolated test table");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quiz_question_seed_loads_idempotently() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let seed = include_str!("../seeds/quiz_questions.sql");
    sqlx::raw_sql(seed)
        .execute(&database.pool)
        .await
        .expect("load quiz question seed");
    sqlx::raw_sql(seed)
        .execute(&database.pool)
        .await
        .expect("reload quiz question seed");

    let counts = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*)::bigint, \
                COUNT(*) FILTER (WHERE quiz_type = 'basic')::bigint, \
                COUNT(*) FILTER (WHERE quiz_type = 'advanced')::bigint \
         FROM quiz_questions",
    )
    .fetch_one(&database.pool)
    .await
    .expect("count seeded questions");
    let option_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quiz_options")
        .fetch_one(&database.pool)
        .await
        .expect("count seeded options");
    let correct_option_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM quiz_options WHERE is_correct")
            .fetch_one(&database.pool)
            .await
            .expect("count seeded correct options");
    let rating_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM question_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("count seeded question ratings");

    assert_eq!(counts, (9, 5, 4));
    assert_eq!(option_count, 34);
    assert_eq!(correct_option_count, 9);
    assert_eq!(rating_count, 9);

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn basic_attempt_can_be_created_then_settled() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let pending = create_pending(
        &database.pool,
        &NewQuizAttempt::new(database.user_id, QuizType::Basic, 1, 500),
    )
    .await
    .expect("create pending basic attempt");
    assert_eq!(pending.quiz_type, "basic");
    assert_eq!(pending.status, "pending");

    let settled = settle_basic_quiz(
        &database.pool,
        QuizSettlementInput::for_attempt(
            pending.id,
            database.user_id,
            vec![QuizAnswer::selected(
                database.question_id,
                database.option_id,
            )],
        ),
    )
    .await
    .expect("settle created basic attempt");

    assert_eq!(settled.attempt.id, pending.id);
    assert_eq!(settled.attempt.status, "completed");
    assert_eq!(settled.attempt.correct_answers, 1);
    assert_eq!(settled.events.len(), 1);
    let event = &settled.events[0];
    assert_eq!(event.attempt_id, Some(pending.id));
    assert_eq!(event.user_id, database.user_id);
    assert_eq!(event.question_id, database.question_id);
    assert_eq!(event.source_type, "quiz_attempt");
    assert_eq!(event.source_id, pending.id);
    assert_eq!(event.quiz_type, "basic");
    assert_eq!(event.outcome, 1);
    assert!(event.correct);
    assert_eq!(event.zone, "basic");
    assert_eq!(event.error_pct, 0.0);
    assert_eq!(event.k, 20);
    assert_eq!(event.sa, 1.0);
    assert_eq!(event.point_delta, 10);
    assert_eq!(event.player_elo_before, 500);
    assert_eq!(event.player_elo_after, 510);
    assert_eq!(event.question_elo_before, 500);
    assert_eq!(event.question_elo_after, 490);
    assert_eq!(event.rating_delta, 10);

    let user_rating = sqlx::query_as::<_, (i32, i32)>(
        "SELECT rating, games_played FROM user_ratings WHERE user_id = $1",
    )
    .bind(database.user_id)
    .fetch_one(&database.pool)
    .await
    .expect("read settled user rating");
    let question_rating = sqlx::query_as::<_, (i32, i32)>(
        "SELECT rating, attempts FROM question_ratings WHERE question_id = $1",
    )
    .bind(database.question_id)
    .fetch_one(&database.pool)
    .await
    .expect("read settled question rating");
    assert_eq!(user_rating, (510, 1));
    assert_eq!(question_rating, (490, 1));

    let completed = find_completed_by_id(&database.pool, pending.id, database.user_id)
        .await
        .expect("query completed basic attempt")
        .expect("completed basic attempt exists");
    assert_eq!(completed.status, "completed");

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advanced_attempt_can_remain_pending_until_settlement() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let advanced_question_id = Uuid::new_v4();
    let advanced_option_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO quiz_questions (id, quiz_type, category, question_text) \
         VALUES ($1, 'advanced', 'test', 'Pending advanced question')",
    )
    .bind(advanced_question_id)
    .execute(&database.pool)
    .await
    .expect("insert advanced test question");
    sqlx::query(
        "INSERT INTO quiz_options (id, question_id, option_text, position, is_correct) \
         VALUES ($1, $2, 'correct', 0, TRUE)",
    )
    .bind(advanced_option_id)
    .bind(advanced_question_id)
    .execute(&database.pool)
    .await
    .expect("insert advanced test option");
    sqlx::query("INSERT INTO question_ratings (question_id) VALUES ($1)")
        .bind(advanced_question_id)
        .execute(&database.pool)
        .await
        .expect("insert advanced question rating");

    let pending = create_pending(
        &database.pool,
        &NewQuizAttempt::new(database.user_id, QuizType::Advanced, 1, 500),
    )
    .await
    .expect("create pending advanced attempt");
    let pending_attempts = pending_advanced_by_user_id(&database.pool, database.user_id, 10, 0)
        .await
        .expect("query pending advanced attempts");
    assert_eq!(pending_attempts.len(), 1);
    assert_eq!(pending_attempts[0].id, pending.id);
    assert_eq!(pending_attempts[0].status, "pending");

    let settled = settle_advanced_quiz(
        &database.pool,
        QuizSettlementInput::for_attempt(
            pending.id,
            database.user_id,
            vec![QuizAnswer::selected(
                advanced_question_id,
                advanced_option_id,
            )],
        ),
    )
    .await
    .expect("settle pending advanced attempt");
    assert_eq!(settled.attempt.id, pending.id);
    assert_eq!(settled.attempt.quiz_type, "advanced");
    assert_eq!(settled.attempt.status, "completed");

    let remaining = pending_advanced_by_user_id(&database.pool, database.user_id, 10, 0)
        .await
        .expect("query remaining pending advanced attempts");
    assert!(remaining.is_empty());

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_concurrent_settlement_is_idempotent() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let attempt_id = Uuid::new_v4();
    let input = QuizSettlementInput::for_attempt(
        attempt_id,
        database.user_id,
        vec![QuizAnswer::selected(
            database.question_id,
            database.option_id,
        )],
    );

    let first = settle_basic_quiz(&database.pool, input.clone());
    let second = settle_basic_quiz(&database.pool, input);
    let (first, second) = tokio::join!(first, second);
    first.expect("first settlement");
    second.expect("duplicate settlement");

    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quiz_attempts")
        .fetch_one(&database.pool)
        .await
        .expect("count attempts");
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_events")
        .fetch_one(&database.pool)
        .await
        .expect("count events");
    assert_eq!(attempts, 1);
    assert_eq!(events, 1);

    let audit = sqlx::query_as::<
        _,
        (
            String,
            Uuid,
            bool,
            String,
            f64,
            i32,
            f64,
            i32,
            i32,
            i32,
            i32,
            i32,
        ),
    >(
        "SELECT source_type, source_id, correct, zone, error_pct, k, sa, point_delta, \
                player_elo_before, player_elo_after, question_elo_before, question_elo_after \
         FROM rating_events",
    )
    .fetch_one(&database.pool)
    .await
    .expect("read rating audit event");
    assert_eq!(audit.0, "quiz_attempt");
    assert_eq!(audit.1, attempt_id);
    assert!(audit.2);
    assert_eq!(audit.3, "basic");
    assert_eq!(audit.4, 0.0);
    assert_eq!(audit.5, 20);
    assert_eq!(audit.6, 1.0);
    assert_eq!(audit.7, 10);
    assert_eq!(audit.8, 500);
    assert_eq!(audit.9, 510);
    assert_eq!(audit.10, 500);
    assert_eq!(audit.11, 490);

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_attempt_cannot_settle_twice() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let input = QuizSettlementInput::for_attempt(
        Uuid::new_v4(),
        database.user_id,
        vec![QuizAnswer::selected(
            database.question_id,
            database.option_id,
        )],
    );

    settle_basic_quiz(&database.pool, input.clone())
        .await
        .expect("first settlement");
    settle_basic_quiz(&database.pool, input)
        .await
        .expect("idempotent duplicate settlement");

    let attempt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quiz_attempts")
        .fetch_one(&database.pool)
        .await
        .expect("count attempts after duplicate settlement");
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_events")
        .fetch_one(&database.pool)
        .await
        .expect("count events after duplicate settlement");
    let games_played: i64 = sqlx::query_scalar("SELECT games_played::bigint FROM user_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read games after duplicate settlement");
    let question_attempts: i64 =
        sqlx::query_scalar("SELECT attempts::bigint FROM question_ratings")
            .fetch_one(&database.pool)
            .await
            .expect("read question attempts after duplicate settlement");
    let user_rating: i32 = sqlx::query_scalar("SELECT rating FROM user_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read user Elo after duplicate settlement");
    let question_rating: i32 = sqlx::query_scalar("SELECT rating FROM question_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read question Elo after duplicate settlement");

    assert_eq!(attempt_count, 1);
    assert_eq!(event_count, 1);
    assert_eq!(games_played, 1);
    assert_eq!(question_attempts, 1);
    assert_eq!(user_rating, 510);
    assert_eq!(question_rating, 490);

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn distinct_concurrent_settlements_preserve_both_rating_updates() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let input_one = QuizSettlementInput::new(
        database.user_id,
        vec![QuizAnswer::selected(
            database.question_id,
            database.option_id,
        )],
    );
    let input_two = QuizSettlementInput::new(
        database.user_id,
        vec![QuizAnswer::unanswered(database.question_id)],
    );

    let first = settle_basic_quiz(&database.pool, input_one);
    let second = settle_basic_quiz(&database.pool, input_two);
    let (first, second) = tokio::join!(first, second);
    first.expect("first settlement");
    second.expect("second settlement");

    let games: i64 = sqlx::query_scalar("SELECT games_played::bigint FROM user_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read user rating");
    let attempts: i64 = sqlx::query_scalar("SELECT attempts::bigint FROM question_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read question rating");
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_events")
        .fetch_one(&database.pool)
        .await
        .expect("count events");
    let ledger_entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_ledger")
        .fetch_one(&database.pool)
        .await
        .expect("count rating ledger entries");
    assert_eq!(games, 2);
    assert_eq!(attempts, 2);
    assert_eq!(events, 2);
    assert_eq!(ledger_entries, 2);

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_settlement_rolls_back_attempt_ratings_and_audit() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let input = QuizSettlementInput::new(
        database.user_id,
        vec![
            QuizAnswer::selected(database.question_id, database.option_id),
            QuizAnswer::unanswered(database.question_id),
        ],
    );

    assert!(settle_basic_quiz(&database.pool, input).await.is_err());

    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quiz_attempts")
        .fetch_one(&database.pool)
        .await
        .expect("count rolled-back attempts");
    let user_rating_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("count rolled-back user ratings");
    let user_games: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT games_played::bigint FROM user_ratings LIMIT 1), 0)",
    )
    .fetch_one(&database.pool)
    .await
    .expect("read rolled-back user rating");
    let question_attempts: i64 =
        sqlx::query_scalar("SELECT attempts::bigint FROM question_ratings")
            .fetch_one(&database.pool)
            .await
            .expect("read rolled-back question rating");
    let question_rating: i32 = sqlx::query_scalar("SELECT rating FROM question_ratings")
        .fetch_one(&database.pool)
        .await
        .expect("read rolled-back question Elo");
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_events")
        .fetch_one(&database.pool)
        .await
        .expect("count rolled-back events");

    assert_eq!(attempts, 0);
    assert_eq!(user_rating_rows, 0);
    assert_eq!(user_games, 0);
    assert_eq!(question_attempts, 0);
    assert_eq!(question_rating, 500);
    assert_eq!(events, 0);

    database.cleanup().await;
}
