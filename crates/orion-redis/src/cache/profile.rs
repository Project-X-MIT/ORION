//! Disposable cache adapter for the public profile read model.
//!
//! A cache hit is useful only when the versioned payload is valid.  The API
//! always falls back to PostgreSQL on a miss, malformed value, version drift,
//! or Redis outage, and rating events can invalidate the user key after their
//! authoritative transaction commits.

use std::future::Future;

use orion_domain::{EventEnvelope, ProfileDto, RatingUpdatedV1, UserId, PROFILE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{RedisClient, RedisClientError, RedisKey};

pub const PROFILE_CACHE_TTL_SECONDS: i64 = 120;

#[derive(Debug, Error)]
pub enum ProfileCacheError {
    #[error("profile cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("profile cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("profile cache schema version is unsupported")]
    UnsupportedSchemaVersion,
    #[error("profile cache user identity does not match the requested key")]
    WrongUser,
    #[error("profile event contract is invalid")]
    EventContract(#[from] orion_domain::ContractError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CachedProfile {
    schema_version: u16,
    profile: ProfileDto,
}

impl CachedProfile {
    fn from_profile(profile: ProfileDto) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile,
        }
    }
}

#[derive(Clone)]
pub struct ProfileCache {
    client: RedisClient,
}

impl ProfileCache {
    #[must_use]
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    pub async fn get(&self, user_id: UserId) -> Result<Option<ProfileDto>, ProfileCacheError> {
        let key = profile_key(user_id);
        let Some(payload) = self.client.get(key.clone()).await? else {
            return Ok(None);
        };
        let cached = match serde_json::from_str::<CachedProfile>(&payload) {
            Ok(value) => value,
            Err(error) => {
                let _ = self.client.delete(key).await;
                return Err(error.into());
            }
        };
        if cached.schema_version != PROFILE_SCHEMA_VERSION
            || cached.profile.schema_version != PROFILE_SCHEMA_VERSION
            || cached.profile.user_id != user_id
        {
            let _ = self.client.delete(key).await;
            return Err(ProfileCacheError::UnsupportedSchemaVersion);
        }
        Ok(Some(cached.profile))
    }

    pub async fn put(&self, user_id: UserId, profile: ProfileDto) -> Result<(), ProfileCacheError> {
        if profile.user_id != user_id || profile.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ProfileCacheError::WrongUser);
        }
        let payload = serde_json::to_string(&CachedProfile::from_profile(profile))?;
        self.client
            .set_ex(profile_key(user_id), payload, PROFILE_CACHE_TTL_SECONDS)
            .await?;
        Ok(())
    }

    pub async fn invalidate(&self, user_id: UserId) -> Result<(), ProfileCacheError> {
        self.client.delete(profile_key(user_id)).await?;
        Ok(())
    }

    /// Invalidates only after a validated, committed rating event.  The event
    /// carries the affected user ID, so no broad cache flush is needed.
    pub async fn on_rating_updated(
        &self,
        event: &EventEnvelope<RatingUpdatedV1>,
    ) -> Result<(), ProfileCacheError> {
        event.validate_contract()?;
        self.invalidate(event.payload.user_id).await
    }

    /// Cache-first profile loading that treats Redis as disposable.
    pub async fn get_or_load<F, Fut, E>(&self, user_id: UserId, loader: F) -> Result<ProfileDto, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<ProfileDto, E>>,
    {
        if let Ok(Some(profile)) = self.get(user_id).await {
            return Ok(profile);
        }
        let profile = loader().await?;
        let _ = self.put(user_id, profile.clone()).await;
        Ok(profile)
    }
}

fn profile_key(user_id: UserId) -> String {
    RedisKey::Profile { user_id }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use uuid::Uuid;

    fn sample() -> ProfileDto {
        ProfileDto {
            schema_version: PROFILE_SCHEMA_VERSION,
            user_id: UserId::from_uuid(Uuid::nil()),
            username: "orion".to_owned(),
            display_name: None,
            bio: None,
            avatar_url: None,
            rating: None,
            global_rank: None,
            rank_movement: None,
            quizzes_completed: 0,
            correct_answers: 0,
            rating_history: Vec::new(),
            rank_history: Vec::new(),
            performance_history: Vec::new(),
            published_research: Vec::new(),
        }
    }

    #[test]
    fn cache_payload_is_versioned_and_public_only() {
        let value = serde_json::to_value(CachedProfile::from_profile(sample())).unwrap();
        assert_eq!(value["schema_version"], Value::from(1));
        assert!(value.get("email").is_none());
        assert!(value["profile"].get("password_hash").is_none());
    }

    #[test]
    fn profile_key_uses_the_registered_ttl_namespace() {
        assert_eq!(
            profile_key(UserId::from_uuid(Uuid::nil())),
            "orion:v1:cache:profile:00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(PROFILE_CACHE_TTL_SECONDS, 120);
    }
}
