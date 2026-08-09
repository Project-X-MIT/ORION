use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{NewUser, User, UserStatus};

const USER_COLUMNS: &str = r#"
    id, email::text AS email, username::text AS username, password_hash,
    display_name, bio, avatar_url, status, email_verified_at, disabled_at,
    deleted_at, created_at, updated_at
"#;

pub async fn create(pool: &PgPool, user: NewUser<'_>) -> Result<User> {
    let query = format!(
        r#"
        WITH created AS (
            INSERT INTO users (email, username, password_hash, display_name)
            VALUES ($1, $2, $3, $4)
            RETURNING {USER_COLUMNS}
        ), initialized_rating AS (
            INSERT INTO user_ratings (user_id)
            SELECT id FROM created
            ON CONFLICT (user_id) DO NOTHING
        )
        SELECT {USER_COLUMNS} FROM created
        "#
    );

    sqlx::query_as::<_, User>(&query)
        .bind(user.email)
        .bind(user.username)
        .bind(user.password_hash)
        .bind(user.display_name)
        .fetch_one(pool)
        .await
}

pub async fn find_by_id(pool: &PgPool, user_id: Uuid) -> Result<Option<User>> {
    let query = format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1 AND status <> 'deleted'");
    sqlx::query_as::<_, User>(&query)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let query = format!(
        "SELECT {USER_COLUMNS} FROM users WHERE email = $1::citext AND status <> 'deleted'"
    );
    sqlx::query_as::<_, User>(&query)
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>> {
    let query = format!(
        "SELECT {USER_COLUMNS} FROM users WHERE username = $1::citext AND status <> 'deleted'"
    );
    sqlx::query_as::<_, User>(&query)
        .bind(username)
        .fetch_optional(pool)
        .await
}

pub async fn update_profile(
    pool: &PgPool,
    user_id: Uuid,
    display_name: Option<&str>,
    bio: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<Option<User>> {
    let query = format!(
        r#"
        UPDATE users
        SET display_name = $2, bio = $3, avatar_url = $4
        WHERE id = $1 AND status <> 'deleted'
        RETURNING {USER_COLUMNS}
        "#
    );
    sqlx::query_as::<_, User>(&query)
        .bind(user_id)
        .bind(display_name)
        .bind(bio)
        .bind(avatar_url)
        .fetch_optional(pool)
        .await
}

pub async fn set_status(pool: &PgPool, user_id: Uuid, status: UserStatus) -> Result<Option<User>> {
    let query = format!(
        r#"
        UPDATE users
        SET status = $2,
            disabled_at = CASE
                WHEN $2 = 'disabled' THEN COALESCE(disabled_at, CURRENT_TIMESTAMP)
                ELSE NULL
            END,
            deleted_at = CASE
                WHEN $2 = 'deleted' THEN COALESCE(deleted_at, CURRENT_TIMESTAMP)
                ELSE NULL
            END
        WHERE id = $1 AND status <> 'deleted'
        RETURNING {USER_COLUMNS}
        "#
    );
    sqlx::query_as::<_, User>(&query)
        .bind(user_id)
        .bind(status.as_str())
        .fetch_optional(pool)
        .await
}
