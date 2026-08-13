use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use axum::http::HeaderMap;
use orion_redis::{RedisClient, RedisClientError};

const WINDOW_SECONDS: i64 = 900;
const MAX_ATTEMPTS: i64 = 10;

#[derive(Clone)]
pub struct LoginRateLimiter {
    redis: RedisClient,
}

impl LoginRateLimiter {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self { redis }
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
        let key = format!("orion:v1:rate_limit:login:{:016x}", hasher.finish());
        let count = self.redis.increment(&key).await?;
        if count == 1 {
            self.redis.expire(&key, WINDOW_SECONDS).await?;
        }
        Ok(count <= MAX_ATTEMPTS)
    }
}
