use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::DatabaseError,
    models::{NewUser, User, UserStatus},
    queries::users,
};

pub type UserRepositoryError = DatabaseError;

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
            .map_err(DatabaseError::from_sqlx)
    }

    pub async fn find_by_id(&self, user_id: Uuid) -> Result<Option<User>, DatabaseError> {
        users::find_by_id(&self.pool, user_id)
            .await
            .map_err(DatabaseError::from_sqlx)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, DatabaseError> {
        users::find_by_email(&self.pool, email)
            .await
            .map_err(DatabaseError::from_sqlx)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, DatabaseError> {
        users::find_by_username(&self.pool, username)
            .await
            .map_err(DatabaseError::from_sqlx)
    }

    pub async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Option<User>, DatabaseError> {
        users::update_profile(&self.pool, user_id, display_name, bio, avatar_url)
            .await
            .map_err(DatabaseError::from_sqlx)
    }

    pub async fn set_status(
        &self,
        user_id: Uuid,
        status: UserStatus,
    ) -> Result<Option<User>, DatabaseError> {
        users::set_status(&self.pool, user_id, status)
            .await
            .map_err(DatabaseError::from_sqlx)
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}
