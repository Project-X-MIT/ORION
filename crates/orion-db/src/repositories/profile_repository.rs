use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::{
    models::{Profile, ProfileStatistics},
    queries::profile,
};

/// Read-only access to public user profiles.
#[derive(Debug, Clone)]
pub struct ProfileRepository {
    pool: PgPool,
}

impl ProfileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the profile belonging to `user_id`, if that user exists.
    pub async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<Profile>> {
        profile::find_by_user_id(&self.pool, user_id).await
    }

    /// Returns the profile for an exact username match, if one exists.
    pub async fn find_by_username(&self, username: &str) -> Result<Option<Profile>> {
        profile::find_by_username(&self.pool, username).await
    }

    /// Returns aggregate profile statistics, if the user exists.
    pub async fn statistics_by_user_id(&self, user_id: Uuid) -> Result<Option<ProfileStatistics>> {
        profile::statistics_by_user_id(&self.pool, user_id).await
    }

    /// Returns the latest Elo persisted for a user, if rating data exists.
    pub async fn current_elo_by_user_id(&self, user_id: Uuid) -> Result<Option<i32>> {
        profile::current_elo_by_user_id(&self.pool, user_id).await
    }

    /// Returns the user's global rank, if rating data exists.
    pub async fn current_rank_by_user_id(&self, user_id: Uuid) -> Result<Option<i64>> {
        profile::current_rank_by_user_id(&self.pool, user_id).await
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
