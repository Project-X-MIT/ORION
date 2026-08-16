use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use axum::http::HeaderMap;
use orion_redis::{RedisClient, RedisClientError, RedisKey, RedisRateLimiter};

const WINDOW_SECONDS: i64 = 900;
const MAX_ATTEMPTS: i64 = 10;

#[derive(Clone)]
pub struct LoginRateLimiter {
    limiter: RedisRateLimiter,
}

impl LoginRateLimiter {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self {
            limiter: RedisRateLimiter::new(redis),
        }
    }

    pub async fn allow(
        &self,
        headers: &HeaderMap,
        normalized_email: &str,
    ) -> Result<bool, RedisClientError> {
        let address = headers
            .get("x-forwarded-for")
            .or_else(|| headers.get("x-real-ip"))
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown");
        let mut hasher = DefaultHasher::new();
        address.hash(&mut hasher);
        normalized_email.hash(&mut hasher);
        let key = RedisKey::LoginRateLimit {
            subject_hash: format!("{:016x}", hasher.finish()),
        };
        self.limiter
            .check(
                key.to_string(),
                MAX_ATTEMPTS as u64,
                std::time::Duration::from_secs(WINDOW_SECONDS as u64),
            )
            .await
            .map(|decision| decision.allowed)
    }
}
