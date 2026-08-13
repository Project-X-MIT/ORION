use std::{env, error::Error};

use orion_db::{
    models::{QuizAnswer, QuizSettlementInput},
    pool as db_pool,
    repositories::QuizRepository,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
    user_id: Uuid,
    question_id: Uuid,
    correct_option_id: Uuid,
    wrong_option_id: Uuid,
    advanced_question_id: Uuid,
    advanced_option_id: Uuid,
}

impl TestDatabase {
    async fn create() -> Result<Option<Self>, Box<dyn Error>> {
        let database_url = match env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
        {
            Ok(database_url) => database_url,
            Err(_) => {
                eprintln!("Skipping quiz integration test: no PostgreSQL test URL configured");
                return Ok(None);
            }
        };
        let admin = db_pool::connect(&database_url).await?;
        let schema = format!("orion_quiz_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await?;

        let search_path = format!("SET search_path TO {schema}, public");
        let pool_search_path = search_path.clone();
        let pool = PgPoolOptions::new()
            .max_connections(8)
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
            .await?;
        db_pool::migrate(&pool).await?;

        let user_id = Uuid::new_v4();
        let question_id = Uuid::new_v4();
        let correct_option_id = Uuid::new_v4();
        let wrong_option_id = Uuid::new_v4();
        let advanced_question_id = Uuid::new_v4();
        let advanced_option_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, username, password_hash) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(format!("{user_id}@quiz.test"))
        .bind(format!("quiz_{}", user_id.simple()))
        .bind("$argon2id$quiz-integration-test")
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO quiz_questions (id, quiz_type, category, question_text, explanation) \
             VALUES ($1, 'basic', 'science', 'Which answer is correct?', 'Internal explanation')",
        )
        .bind(question_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO quiz_options (id, question_id, option_text, position, is_correct) \
             VALUES ($1, $2, 'Correct', 0, TRUE), ($3, $2, 'Wrong', 1, FALSE)",
        )
        .bind(correct_option_id)
        .bind(question_id)
        .bind(wrong_option_id)
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO question_ratings (question_id) VALUES ($1)")
            .bind(question_id)
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO quiz_questions (id, quiz_type, category, question_text) \
             VALUES ($1, 'advanced', 'test', 'Which prediction is correct?')",
        )
        .bind(advanced_question_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO quiz_options (id, question_id, option_text, position, is_correct) \
             VALUES ($1, $2, 'Correct prediction', 0, TRUE)",
        )
        .bind(advanced_option_id)
        .bind(advanced_question_id)
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO question_ratings (question_id) VALUES ($1)")
            .bind(advanced_question_id)
            .execute(&pool)
            .await?;

        Ok(Some(Self {
            pool,
            admin,
            schema,
            user_id,
            question_id,
            correct_option_id,
            wrong_option_id,
            advanced_question_id,
            advanced_option_id,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn basic_questions_are_projected_and_submission_settles_once() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let repository = QuizRepository::new(database.pool.clone());

    let questions = repository.basic_questions_with_options(10, 0).await?;
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].question.id, database.question_id);
    assert_eq!(questions[0].question.quiz_type, "basic");
    assert_eq!(questions[0].options.len(), 2);
    assert!(questions[0].options.iter().any(|option| option.is_correct));

    let advanced_questions = repository.advanced_questions_with_options(10, 0).await?;
    assert_eq!(advanced_questions.len(), 1);
    assert_eq!(
        advanced_questions[0].question.id,
        database.advanced_question_id
    );
    assert_eq!(advanced_questions[0].question.quiz_type, "advanced");
    assert_eq!(advanced_questions[0].options.len(), 1);

    let attempt_id = Uuid::new_v4();
    let input = QuizSettlementInput::for_attempt(
        attempt_id,
        database.user_id,
        vec![QuizAnswer::selected(
            database.question_id,
            database.correct_option_id,
        )],
    );
    let settled = repository.settle_basic(input.clone()).await?;
    assert_eq!(settled.attempt.id, attempt_id);
    assert_eq!(settled.attempt.status, "completed");
    assert_eq!(settled.attempt.correct_answers, 1);
    assert_eq!(settled.attempt.score, 100);
    assert_eq!(settled.user_rating.rating, 510);
    assert_eq!(settled.events.len(), 1);

    let retried = repository.settle_basic(input).await?;
    assert_eq!(retried.attempt.id, attempt_id);
    assert_eq!(retried.user_rating.rating, 510);
    assert_eq!(retried.events.len(), 1);

    let stored_result = repository
        .find_completed_result(attempt_id, database.user_id)
        .await?
        .expect("completed attempt result exists");
    assert_eq!(stored_result.attempt.id, attempt_id);
    assert_eq!(stored_result.attempt.quiz_type, "basic");
    assert_eq!(stored_result.events.len(), 1);
    assert!(repository
        .find_completed_result(attempt_id, Uuid::new_v4())
        .await?
        .is_none());

    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rating_events WHERE attempt_id = $1",
    )
    .bind(attempt_id)
    .fetch_one(&database.pool)
    .await?;
    assert_eq!(event_count, 1);

    let question_rating: (i32, i32) = sqlx::query_as(
        "SELECT rating, attempts FROM question_ratings WHERE question_id = $1",
    )
    .bind(database.question_id)
    .fetch_one(&database.pool)
    .await?;
    assert_eq!(question_rating, (490, 1));

    let advanced_attempt_id = Uuid::new_v4();
    let advanced_settled = repository
        .settle_advanced(QuizSettlementInput::for_attempt(
            advanced_attempt_id,
            database.user_id,
            vec![QuizAnswer::selected(
                database.advanced_question_id,
                database.advanced_option_id,
            )],
        ))
        .await?;
    assert_eq!(advanced_settled.attempt.id, advanced_attempt_id);
    assert_eq!(advanced_settled.attempt.status, "completed");
    assert_eq!(advanced_settled.attempt.correct_answers, 1);
    assert_eq!(advanced_settled.events[0].k, 40);
    assert!(advanced_settled.user_rating.rating > settled.user_rating.rating);

    let advanced_rating: (i32, i32) = sqlx::query_as(
        "SELECT rating, attempts FROM question_ratings WHERE question_id = $1",
    )
    .bind(database.advanced_question_id)
    .fetch_one(&database.pool)
    .await?;
    assert!(advanced_rating.0 < 500);
    assert_eq!(advanced_rating.1, 1);

    database.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn completed_results_are_scoped_to_the_authenticated_owner() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let repository = QuizRepository::new(database.pool.clone());
    let attempt_id = Uuid::new_v4();

    repository
        .settle_basic(QuizSettlementInput::for_attempt(
            attempt_id,
            database.user_id,
            vec![QuizAnswer::selected(
                database.question_id,
                database.correct_option_id,
            )],
        ))
        .await?;

    assert!(repository
        .find_completed_result(attempt_id, database.user_id)
        .await?
        .is_some());
    assert!(repository
        .find_completed_result(attempt_id, Uuid::new_v4())
        .await?
        .is_none());
    assert!(repository
        .find_completed_basic_attempt(attempt_id, Uuid::new_v4())
        .await?
        .is_none());

    database.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_settlement_ignores_a_tampered_retry_payload() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let repository = QuizRepository::new(database.pool.clone());
    let attempt_id = Uuid::new_v4();

    let first = repository
        .settle_basic(QuizSettlementInput::for_attempt(
            attempt_id,
            database.user_id,
            vec![QuizAnswer::selected(
                database.question_id,
                database.correct_option_id,
            )],
        ))
        .await?;
    let retry = repository
        .settle_basic(QuizSettlementInput::for_attempt(
            attempt_id,
            database.user_id,
            vec![QuizAnswer::selected(
                database.question_id,
                database.wrong_option_id,
            )],
        ))
        .await?;

    assert_eq!(retry.attempt.score, first.attempt.score);
    assert_eq!(retry.attempt.correct_answers, first.attempt.correct_answers);
    assert_eq!(retry.user_rating, first.user_rating);
    assert_eq!(retry.events, first.events);

    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rating_events WHERE attempt_id = $1",
    )
    .bind(attempt_id)
    .fetch_one(&database.pool)
    .await?;
    assert_eq!(event_count, 1);

    database.cleanup().await?;
    Ok(())
}
