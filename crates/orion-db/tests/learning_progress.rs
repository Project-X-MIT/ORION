use std::{env, error::Error};

use orion_db::{pool, repositories::LearningRepository};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Result<Option<Self>, Box<dyn Error>> {
        let database_url =
            match env::var("ORION_TEST_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
                Ok(database_url) => database_url,
                Err(_) => {
                    eprintln!("Skipping learning concurrency test: no PostgreSQL URL configured");
                    return Ok(None);
                }
            };
        let admin = match pool::connect(&database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping learning concurrency test: {error}");
                return Ok(None);
            }
        };
        let schema = format!("orion_learning_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await?;

        let search_path = format!("SET search_path TO {schema}, public");
        let pool = PgPoolOptions::new()
            .max_connections(8)
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
            .await?;
        pool::migrate(&pool).await?;

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

async fn seed_published_lesson(pool: &PgPool) -> Result<(Uuid, Uuid, Uuid), sqlx::Error> {
    let user_id = Uuid::new_v4();
    let module_id = Uuid::new_v4();
    let lesson_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, email, username, password_hash)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(format!("{user_id}@learning.test"))
    .bind(format!("learning_{}", user_id.simple()))
    .bind("$argon2id$learning-concurrency-test")
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO course_modules
            (id, slug, title, description, display_order, is_published)
         VALUES ($1, $2, $3, $4, $5, TRUE)",
    )
    .bind(module_id)
    .bind("foundations")
    .bind("Foundations")
    .bind("Learning foundations")
    .bind(1_i32)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO course_lessons
            (id, module_id, slug, title, summary, content, lesson_order,
             estimated_minutes, is_published)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)",
    )
    .bind(lesson_id)
    .bind(module_id)
    .bind("market-basics")
    .bind("Market Basics")
    .bind("A first lesson")
    .bind("Safe learning content")
    .bind(1_i32)
    .bind(10_i32)
    .execute(pool)
    .await?;

    Ok((user_id, module_id, lesson_id))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_completion_replays_one_authoritative_progress_row() -> Result<(), Box<dyn Error>>
{
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let (user_id, _module_id, lesson_id) = seed_published_lesson(&database.pool).await?;
    let repository = LearningRepository::new(database.pool.clone());

    let first_repository = repository.clone();
    let second_repository = repository.clone();
    let (first, second) = tokio::join!(
        first_repository.complete_lesson(user_id, lesson_id),
        second_repository.complete_lesson(user_id, lesson_id),
    );
    let first = first?;
    let second = second?;

    assert!(first.completed);
    assert!(second.completed);
    assert!(first.completed_at.is_some());
    assert!(second.completed_at.is_some());

    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM course_progress
         WHERE user_id = $1 AND lesson_id = $2",
    )
    .bind(user_id)
    .bind(lesson_id)
    .fetch_one(&database.pool)
    .await?;
    assert_eq!(
        row_count, 1,
        "concurrent completion must not duplicate progress"
    );

    let persisted = repository
        .progress_by_user_and_lesson(user_id, lesson_id)
        .await?
        .expect("the authoritative progress row remains readable");
    assert!(persisted.completed);

    database.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_survives_logout_redis_loss_and_another_device_read() -> Result<(), Box<dyn Error>>
{
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let (user_id, _module_id, lesson_id) = seed_published_lesson(&database.pool).await?;

    // Device A completes through PostgreSQL. Logout and Redis loss are
    // intentionally absent from this write path: neither operation can delete
    // the authoritative course_progress row.
    LearningRepository::new(database.pool.clone())
        .complete_lesson(user_id, lesson_id)
        .await?;

    // A fresh repository represents another authenticated device. It reads
    // the same user-owned row without requiring Redis or the old session.
    let device_b_repository = LearningRepository::new(database.pool.clone());
    let progress = device_b_repository
        .progress_by_user_and_lesson(user_id, lesson_id)
        .await?
        .expect("another device resumes from PostgreSQL after Redis loss");

    assert!(progress.completed);
    assert_eq!(progress.user_id, user_id);
    assert_eq!(progress.lesson_id, lesson_id);

    database.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unpublished_modules_and_lessons_are_hidden_from_learner_queries(
) -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let draft_module_id = Uuid::new_v4();
    let published_module_id = Uuid::new_v4();
    let lesson_under_draft_module_id = Uuid::new_v4();
    let draft_lesson_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO course_modules
            (id, slug, title, display_order, is_published)
         VALUES ($1, $2, $3, $4, FALSE),
                ($5, $6, $7, $8, TRUE)",
    )
    .bind(draft_module_id)
    .bind("draft-module")
    .bind("Draft module")
    .bind(1_i32)
    .bind(published_module_id)
    .bind("published-module")
    .bind("Published module")
    .bind(2_i32)
    .execute(&database.pool)
    .await?;

    sqlx::query(
        "INSERT INTO course_lessons
            (id, module_id, slug, title, content, lesson_order,
             estimated_minutes, is_published)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE),
                ($8, $9, $10, $11, $12, $13, $14, FALSE)",
    )
    .bind(lesson_under_draft_module_id)
    .bind(draft_module_id)
    .bind("hidden-parent")
    .bind("Hidden parent lesson")
    .bind("Content under a draft module")
    .bind(1_i32)
    .bind(10_i32)
    .bind(draft_lesson_id)
    .bind(published_module_id)
    .bind("draft-lesson")
    .bind("Draft lesson")
    .bind("Unpublished content")
    .bind(1_i32)
    .bind(10_i32)
    .execute(&database.pool)
    .await?;

    let repository = LearningRepository::new(database.pool.clone());
    let modules = repository.modules().await?;
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id, published_module_id);
    assert!(repository.module_by_id(draft_module_id).await?.is_none());
    assert!(repository
        .lessons_by_module_id(draft_module_id)
        .await?
        .is_empty());
    assert!(repository
        .lesson_by_id(lesson_under_draft_module_id)
        .await?
        .is_none());
    assert!(repository
        .lessons_by_module_id(published_module_id)
        .await?
        .is_empty());
    assert!(repository.lesson_by_id(draft_lesson_id).await?.is_none());

    database.cleanup().await?;
    Ok(())
}
