use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{NewUser, User, UserStatus},
    queries::users,
};

#[derive(Debug, Error)]
pub enum UserRepositoryError {
    #[error("email is already registered")]
    DuplicateEmail,
    #[error("username is already registered")]
    DuplicateUsername,
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
}

impl UserRepositoryError {
    fn from_sqlx(error: sqlx::Error) -> Self {
        let constraint = error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::constraint);
        match constraint {
            Some("users_email_unique_idx") => Self::DuplicateEmail,
            Some("users_username_unique_idx" | "users_username_key") => Self::DuplicateUsername,
            _ => Self::Database(error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: NewUser<'_>) -> Result<User, UserRepositoryError> {
        users::create(&self.pool, user)
            .await
            .map_err(UserRepositoryError::from_sqlx)
    }

    pub async fn find_by_id(&self, user_id: Uuid) -> Result<Option<User>, sqlx::Error> {
        users::find_by_id(&self.pool, user_id).await
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        users::find_by_email(&self.pool, email).await
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        users::find_by_username(&self.pool, username).await
    }

    pub async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Option<User>, sqlx::Error> {
        users::update_profile(&self.pool, user_id, display_name, bio, avatar_url).await
    }

    pub async fn set_status(
        &self,
        user_id: Uuid,
        status: UserStatus,
    ) -> Result<Option<User>, sqlx::Error> {
        users::set_status(&self.pool, user_id, status).await
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}
