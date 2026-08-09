use std::env;

use orion_db::{
    models::{NewNotification, NewUser},
    pool,
    queries::notifications,
    repositories::{UserRepository, UserRepositoryError},
    transactions::{
        create_notification, list_notifications, mark_notification_read, unread_notification_count,
    },
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
                eprintln!("Skipping PostgreSQL DB-01 test: {error}");
                return None;
            }
        };
        let schema = format!("orion_db01_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated DB-01 schema");

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
            .expect("connect isolated DB-01 pool");

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
            .expect("drop isolated DB-01 schema");
        self.admin.close().await;
    }
}

#[tokio::test]
async fn fresh_chain_users_notifications_and_seed_are_repeat_safe() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    pool::migrate(&database.pool)
        .await
        .expect("apply complete migration chain");
    pool::health_check(&database.pool)
        .await
        .expect("validate database health");

    for table in [
        "users",
        "user_ratings",
        "quiz_attempts",
        "leaderboard_rank_history",
        "research_papers",
        "news_articles",
        "course_progress",
        "notifications",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&database.pool)
            .await
            .expect("check migrated feature table");
        assert!(exists, "missing migrated table {table}");
    }

    let users = UserRepository::new(database.pool.clone());
    let user = users
        .create(NewUser {
            email: "user@example.com",
            username: "contract_user",
            password_hash: "$argon2id$test-only-hash",
            display_name: Some("Contract User"),
        })
        .await
        .expect("create user and initial rating");
    assert_eq!(
        users
            .find_by_email("USER@example.com")
            .await
            .expect("find user")
            .expect("user exists")
            .id,
        user.id
    );

    let duplicate_email = users
        .create(NewUser {
            email: "USER@example.com",
            username: "different_user",
            password_hash: "$argon2id$test-only-hash",
            display_name: None,
        })
        .await;
    assert!(matches!(
        duplicate_email,
        Err(UserRepositoryError::DuplicateEmail)
    ));

    let duplicate_username = users
        .create(NewUser {
            email: "different@example.com",
            username: "CONTRACT_USER",
            password_hash: "$argon2id$test-only-hash",
            display_name: None,
        })
        .await;
    assert!(matches!(
        duplicate_username,
        Err(UserRepositoryError::DuplicateUsername)
    ));

    let input = NewNotification {
        user_id: user.id,
        kind: "system",
        title: "Welcome",
        body: "Welcome to ORION.",
        action_url: Some("/learning"),
        deduplication_key: "welcome:v1",
        expires_at: None,
    };
    let notification = create_notification(&database.pool, input)
        .await
        .expect("create notification transaction");
    let duplicate = notifications::create(&database.pool, input)
        .await
        .expect("deduplicate notification retry");
    assert_eq!(notification.id, duplicate.id);
    assert_eq!(
        unread_notification_count(&database.pool, user.id)
            .await
            .expect("count unread"),
        1
    );

    let first_read = mark_notification_read(&database.pool, user.id, notification.id)
        .await
        .expect("mark read")
        .expect("notification exists");
    let repeated_read = mark_notification_read(&database.pool, user.id, notification.id)
        .await
        .expect("repeat mark read")
        .expect("notification still exists");
    assert_eq!(first_read.read_at, repeated_read.read_at);
    assert_eq!(
        unread_notification_count(&database.pool, user.id)
            .await
            .expect("count read state"),
        0
    );
    assert_eq!(
        list_notifications(&database.pool, user.id, 20, 0)
            .await
            .expect("list notifications")
            .len(),
        1
    );

    let seed = include_str!("../seeds/dev_users.sql");
    sqlx::raw_sql(seed)
        .execute(&database.pool)
        .await
        .expect("execute development seed");
    sqlx::raw_sql(seed)
        .execute(&database.pool)
        .await
        .expect("repeat development seed");
    let seed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE email IN ('admin@orion.local', 'learner@orion.local')",
    )
    .fetch_one(&database.pool)
    .await
    .expect("count development users");
    assert_eq!(seed_count, 2);

    database.cleanup().await;
}

#[tokio::test]
async fn upgrade_accepts_recorded_empty_legacy_migrations() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    sqlx::raw_sql(
        r#"
        CREATE TABLE users (id UUID PRIMARY KEY);
        INSERT INTO users (id) VALUES ('00000000-0000-0000-0000-000000000099');
        CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        );
        INSERT INTO _sqlx_migrations (
            version, description, success, checksum, execution_time
        ) VALUES
            (
                202608070001,
                'extensions',
                TRUE,
                decode(
                    '38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b',
                    'hex'
                ),
                0
            ),
            (
                202608070002,
                'users',
                TRUE,
                decode(
                    '38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b',
                    'hex'
                ),
                0
            );
        "#,
    )
    .execute(&database.pool)
    .await
    .expect("record immutable empty legacy migrations");

    pool::migrate(&database.pool)
        .await
        .expect("upgrade legacy database");
    let has_email: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'users'
              AND column_name = 'email'
        )",
    )
    .fetch_one(&database.pool)
    .await
    .expect("inspect upgraded users table");
    assert!(has_email);
    let legacy_username: String = sqlx::query_scalar(
        "SELECT username::text FROM users
         WHERE id = '00000000-0000-0000-0000-000000000099'",
    )
    .fetch_one(&database.pool)
    .await
    .expect("read upgraded legacy username");
    assert!(legacy_username.len() <= 32);

    database.cleanup().await;
}
